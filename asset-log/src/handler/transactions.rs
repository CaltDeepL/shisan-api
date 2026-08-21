//! 取引の登録・参照・削除。
//!
//! 登録・削除はどちらも「先に書き換えてから、変更後の全取引を畳み込み直す」方式。
//! 過去日付への差し込みや、買いの削除で後続の売却が成立しなくなるケースも
//! 同じ経路で検出できる。整合しなければトランザクションごと捨てる。

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::position::{PositionError, TradeKind, build_holding},
    error::AppError,
    middleware::auth::AuthUser,
    repository::transaction_repo::{self, NewTransaction, Transaction, TransactionFilter},
    state::AppState,
};

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;

/// 約定日の未来判定は日本時間で行う（UTCだと日本の夜に当日分が弾かれる）。
fn today_jst() -> NaiveDate {
    let jst = FixedOffset::east_opt(9 * 3600).expect("固定オフセットは常に有効");
    Utc::now().with_timezone(&jst).date_naive()
}

#[derive(Debug, Deserialize)]
pub struct CreateTransaction {
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub kind: TradeKind,
    pub quantity: Decimal,
    pub price: Decimal,
    #[serde(default)]
    pub fee: Decimal,
    pub traded_at: NaiveDate,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub account_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id: Uuid,
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub kind: TradeKind,
    pub quantity: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
    pub traded_at: NaiveDate,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<Transaction> for TransactionResponse {
    fn from(t: Transaction) -> Self {
        Self {
            id: t.id,
            account_id: t.account_id,
            asset_id: t.asset_id,
            kind: t.kind,
            quantity: t.quantity,
            price: t.price,
            fee: t.fee,
            traded_at: t.traded_at,
            note: t.note,
            created_at: t.created_at,
        }
    }
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateTransaction>,
) -> Result<(StatusCode, Json<TransactionResponse>), AppError> {
    validate(&payload)?;

    // 口座・銘柄の所有確認と price_unit の取得を1往復で行う
    let ctx = transaction_repo::fetch_position_context(
        &state.db,
        user.0,
        payload.account_id,
        payload.asset_id,
    )
    .await?;

    if !ctx.account_exists {
        return Err(AppError::NotFound("口座"));
    }
    let Some(price_unit) = ctx.price_unit else {
        return Err(AppError::NotFound("銘柄が見つかりません"));
    };

    let mut tx = state.db.begin().await?;
    transaction_repo::lock_position(&mut tx, payload.account_id, payload.asset_id).await?;

    let created = transaction_repo::insert(
        &mut tx,
        &NewTransaction {
            user_id: user.0,
            account_id: payload.account_id,
            asset_id: payload.asset_id,
            kind: payload.kind,
            quantity: payload.quantity,
            price: payload.price,
            fee: payload.fee,
            traded_at: payload.traded_at,
            note: payload.note.map(|n| n.trim().to_owned()),
        },
    )
    .await?;

    // 挿入後の全取引を畳み込み直す。売却超過ならこの取引は無かったことにする
    let trades =
        transaction_repo::fetch_trades(&mut tx, payload.account_id, payload.asset_id).await?;
    if let Err(err) = build_holding(&trades, price_unit) {
        tx.rollback().await?;
        return Err(position_error(err));
    }

    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<TransactionResponse>>, AppError> {
    #[allow(clippy::collapsible_if)]
    if let (Some(from), Some(to)) = (query.from, query.to) {
        if from > to {
            return Err(AppError::field(
                "from",
                "開始日が終了日より後になっています",
            ));
        }
    }

    let filter = TransactionFilter {
        account_id: query.account_id,
        asset_id: query.asset_id,
        from: query.from,
        to: query.to,
        limit: query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
    };

    let rows = transaction_repo::list(&state.db, user.0, &filter).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

pub async fn show(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TransactionResponse>, AppError> {
    let found = transaction_repo::find_by_id(&state.db, user.0, id)
        .await?
        .ok_or(AppError::NotFound("取引が見つかりません"))?;
    Ok(Json(found.into()))
}

pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let target = transaction_repo::find_by_id(&state.db, user.0, id)
        .await?
        .ok_or(AppError::NotFound("取引が見つかりません"))?;

    let ctx = transaction_repo::fetch_position_context(
        &state.db,
        user.0,
        target.account_id,
        target.asset_id,
    )
    .await?;
    let price_unit = ctx
        .price_unit
        .ok_or(AppError::NotFound("銘柄が見つかりません"))?;

    let mut tx = state.db.begin().await?;
    transaction_repo::lock_position(&mut tx, target.account_id, target.asset_id).await?;

    if !transaction_repo::delete(&mut tx, user.0, id).await? {
        // ロック待ちの間に他リクエストが消していた場合
        tx.rollback().await?;
        return Err(AppError::NotFound("取引が見つかりません"));
    }

    let trades =
        transaction_repo::fetch_trades(&mut tx, target.account_id, target.asset_id).await?;
    if let Err(err) = build_holding(&trades, price_unit) {
        tx.rollback().await?;
        return Err(match err {
            PositionError::Oversell { .. } => {
                AppError::unprocessable("この取引を削除すると、以降の売却が保有数量を超えます")
            }
            other => position_error(other),
        });
    }

    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn validate(payload: &CreateTransaction) -> Result<(), AppError> {
    if payload.quantity <= Decimal::ZERO {
        return Err(AppError::field(
            "quantity",
            "数量は正の数を指定してください",
        ));
    }
    if payload.price < Decimal::ZERO {
        return Err(AppError::field("price", "価格は0以上を指定してください"));
    }
    if payload.fee < Decimal::ZERO {
        return Err(AppError::field("fee", "手数料は0以上を指定してください"));
    }
    // current_date は IMMUTABLE でないため CHECK に書けず、ここで弾く
    if payload.traded_at > today_jst() {
        return Err(AppError::field(
            "traded_at",
            "約定日に未来日は指定できません",
        ));
    }
    if payload.note.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(AppError::field("note", "メモは空白のみにできません"));
    }
    Ok(())
}

/// `PositionError` はすべて入力起因なので422に落とす。
/// メッセージは thiserror の Display をそのまま使う。
fn position_error(err: PositionError) -> AppError {
    AppError::unprocessable(err.to_string())
}
