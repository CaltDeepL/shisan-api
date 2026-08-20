//! 取引の永続化。
//!
//! 保有ポジションは **(account_id, asset_id) 単位**で畳み込む。
//! NISA口座と特定口座で同じ銘柄を持った場合、取得単価は口座ごとに独立する。
//!
//! 整合性検証の方針:
//! 1. `lock_position` で (account_id, asset_id) にアドバイザリロックを取る
//! 2. INSERT / DELETE を実行する
//! 3. `fetch_trades` で**変更後の全取引**を取り直し、`domain::position::build_holding` に通す
//! 4. `Oversell` ならトランザクションをロールバックして 422
//!
//! `FOR UPDATE` は既存行しかロックできず同時 INSERT を防げないため、
//! 行ロックではなくアドバイザリロックを使う。

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::position::{Trade, TradeKind};

/// 取引1件（DBの行）。
#[derive(Debug, Clone)]
pub struct Transaction {
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

/// 登録するときの入力。
#[derive(Debug, Clone)]
pub struct NewTransaction {
    pub user_id: Uuid,
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub kind: TradeKind,
    pub quantity: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
    pub traded_at: NaiveDate,
    pub note: Option<String>,
}

/// 一覧の絞り込み条件。`None` はその条件を使わない。
#[derive(Debug, Default, Clone)]
pub struct TransactionFilter {
    pub account_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub limit: i64,
}

/// 口座・銘柄の所有確認の結果。両方とも自分のものでなければ 404 にする。
#[derive(Debug)]
pub struct PositionContext {
    pub account_exists: bool,
    /// 銘柄が自分のものなら `assets.price_unit`。無ければ `None`。
    pub price_unit: Option<Decimal>,
}

/// 口座と銘柄の所有を1往復で確認し、あわせて price_unit を取る。
///
/// `price_unit` は `build_holding` に渡すため、どのみち必要になる。
pub async fn fetch_position_context(
    db: &PgPool,
    user_id: Uuid,
    account_id: Uuid,
    asset_id: Uuid,
) -> Result<PositionContext, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            EXISTS (SELECT 1 FROM accounts WHERE id = $2 AND user_id = $1) AS "account_exists!",
            (SELECT price_unit FROM assets WHERE id = $3 AND user_id = $1) AS "price_unit?"
        "#,
        user_id,
        account_id,
        asset_id,
    )
    .fetch_one(db)
    .await?;

    Ok(PositionContext {
        account_exists: row.account_exists,
        price_unit: row.price_unit,
    })
}

/// (account_id, asset_id) にトランザクション内アドバイザリロックを取る。
///
/// コミット／ロールバックで自動解放される。同じ組み合わせへの同時登録だけが直列化され、
/// 別銘柄・別口座への登録はブロックされない。
pub async fn lock_position(
    conn: &mut PgConnection,
    account_id: Uuid,
    asset_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"SELECT pg_advisory_xact_lock(hashtextextended($1::uuid::text || ':' || $2::uuid::text, 0))"#,
        account_id,
        asset_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// 畳み込み用に、そのポジションの全取引を約定日昇順で取る。
///
/// 並び順は `transactions_position_idx` と同じ (traded_at, created_at, id)。
/// 同日の複数取引は登録順に確定する。
pub async fn fetch_trades(
    conn: &mut PgConnection,
    account_id: Uuid,
    asset_id: Uuid,
) -> Result<Vec<Trade>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT kind AS "kind: TradeKind", quantity, price, fee
        FROM transactions
        WHERE account_id = $1 AND asset_id = $2
        ORDER BY traded_at, created_at, id
        "#,
        account_id,
        asset_id,
    )
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Trade {
            kind: r.kind,
            quantity: r.quantity,
            price: r.price,
            fee: r.fee,
        })
        .collect())
}

pub async fn insert(
    conn: &mut PgConnection,
    new: &NewTransaction,
) -> Result<Transaction, sqlx::Error> {
    sqlx::query_as!(
        Transaction,
        r#"
        INSERT INTO transactions
            (user_id, account_id, asset_id, kind, quantity, price, fee, traded_at, note)
        VALUES ($1, $2, $3, $4::trade_kind, $5, $6, $7, $8, $9)
        RETURNING
            id, account_id, asset_id, kind AS "kind: TradeKind",
            quantity, price, fee, traded_at, note, created_at
        "#,
        new.user_id,
        new.account_id,
        new.asset_id,
        new.kind as TradeKind,
        new.quantity,
        new.price,
        new.fee,
        new.traded_at,
        new.note.as_deref(),
    )
    .fetch_one(conn)
    .await
}

pub async fn find_by_id(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<Transaction>, sqlx::Error> {
    sqlx::query_as!(
        Transaction,
        r#"
        SELECT
            id, account_id, asset_id, kind AS "kind: TradeKind",
            quantity, price, fee, traded_at, note, created_at
        FROM transactions
        WHERE id = $1 AND user_id = $2
        "#,
        id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// 一覧。NULL 渡しでその条件を無効化する形にして、マクロ版クエリを1本で済ませる。
pub async fn list(
    db: &PgPool,
    user_id: Uuid,
    filter: &TransactionFilter,
) -> Result<Vec<Transaction>, sqlx::Error> {
    sqlx::query_as!(
        Transaction,
        r#"
        SELECT
            id, account_id, asset_id, kind AS "kind: TradeKind",
            quantity, price, fee, traded_at, note, created_at
        FROM transactions
        WHERE user_id = $1
          AND ($2::uuid IS NULL OR account_id = $2)
          AND ($3::uuid IS NULL OR asset_id   = $3)
          AND ($4::date IS NULL OR traded_at >= $4)
          AND ($5::date IS NULL OR traded_at <= $5)
        ORDER BY traded_at DESC, created_at DESC, id DESC
        LIMIT $6
        "#,
        user_id,
        filter.account_id,
        filter.asset_id,
        filter.from,
        filter.to,
        filter.limit,
    )
    .fetch_all(db)
    .await
}

/// 削除。消せたら true。他人の取引は 0 件更新になり false を返す（＝404）。
pub async fn delete(conn: &mut PgConnection, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM transactions WHERE id = $1 AND user_id = $2",
        id,
        user_id,
    )
    .execute(conn)
    .await?;

    Ok(result.rows_affected() > 0)
}
