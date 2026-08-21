//! 保有ポジション算出のための読み取り専用クエリ。
//!
//! `/holdings` は (account_id, asset_id) ごとの N+1 を避けるため、
//! 取引を1本の SELECT で全件取り、畳み込みは service 層 (Rust 側) で行う。

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

// NOTE: AccountType / AssetClass の実際のモジュールパスは既存コードに合わせて調整すること。
use crate::domain::account::AccountType;
use crate::domain::asset::AssetClass;
use crate::domain::position::TradeKind;

/// 取引1件と、保有一覧の表示に必要な口座・銘柄のメタ情報。
///
/// `price_unit` は `domain::position::{build_holding, evaluate}` の引数になるため、
/// 取引と同じクエリで必ず持ってくる。
#[derive(Debug, Clone)]
pub struct TradeRow {
    pub account_id: Uuid,
    pub account_name: String,
    pub account_type: AccountType,
    pub asset_id: Uuid,
    pub symbol: String,
    pub asset_name: String,
    pub asset_class: AssetClass,
    pub currency: String,
    pub price_unit: Decimal,
    pub kind: TradeKind,
    pub quantity: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
}

/// 銘柄ごとの最新価格。
#[derive(Debug, Clone)]
pub struct LatestPriceRow {
    pub asset_id: Uuid,
    pub price: Decimal,
    pub priced_on: NaiveDate,
}

/// ユーザーの全取引を、口座・銘柄のメタ情報つきで取得する。
///
/// `ORDER BY` は `transactions_position_idx`
/// (account_id, asset_id, traded_at, created_at, id) と同じ並び。
/// `build_holding` は約定日昇順を前提にしているので、この順序がそのまま畳み込みの入力になる。
///
/// `account_id` に `Some` を渡すとその口座のみに絞る。
/// 口座の存在確認は呼び出し側の責務（`account_exists` を先に呼ぶ）。
pub async fn fetch_trades_for_holdings(
    db: &PgPool,
    user_id: Uuid,
    account_id: Option<Uuid>,
) -> Result<Vec<TradeRow>, sqlx::Error> {
    sqlx::query_as!(
        TradeRow,
        r#"
        SELECT
            t.account_id   AS "account_id!",
            a.name         AS "account_name!",
            a.account_type AS "account_type!: AccountType",
            t.asset_id     AS "asset_id!",
            s.symbol       AS "symbol!",
            s.name         AS "asset_name!",
            s.asset_class  AS "asset_class!: AssetClass",
            s.currency     AS "currency!",
            s.price_unit   AS "price_unit!",
            t.kind         AS "kind!: TradeKind",
            t.quantity     AS "quantity!",
            t.price        AS "price!",
            t.fee          AS "fee!"
        FROM transactions t
        JOIN accounts a ON a.id = t.account_id
        JOIN assets   s ON s.id = t.asset_id
        WHERE t.user_id = $1
          AND ($2::uuid IS NULL OR t.account_id = $2)
        ORDER BY t.account_id, t.asset_id, t.traded_at, t.created_at, t.id
        "#,
        user_id,
        account_id,
    )
    .fetch_all(db)
    .await
}

/// ユーザーが保有しうる全銘柄について、最新日の価格を1件ずつ取得する。
///
/// `asset_prices` は user_id を持たない（タスク#6の設計）ため、`assets` 経由で絞る。
/// `DISTINCT ON (asset_id)` + `ORDER BY asset_id, priced_on DESC` で銘柄ごとに最新の1行だけが残る。
/// 価格が1件も無い銘柄はこの結果に現れない（呼び出し側で `None` として扱う）。
pub async fn fetch_latest_prices(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<LatestPriceRow>, sqlx::Error> {
    sqlx::query_as!(
        LatestPriceRow,
        r#"
        SELECT DISTINCT ON (p.asset_id)
            p.asset_id  AS "asset_id!",
            p.price     AS "price!",
            p.priced_on AS "priced_on!"
        FROM asset_prices p
        JOIN assets s ON s.id = p.asset_id
        WHERE s.user_id = $1
        ORDER BY p.asset_id, p.priced_on DESC
        "#,
        user_id,
    )
    .fetch_all(db)
    .await
}

/// `?account_id=` で指定された口座が、そのユーザーのものとして存在するか。
///
/// 他人の口座・存在しない口座はどちらも `false` を返し、呼び出し側で 404 に落とす
/// （タスク#5 で決めた「他人のリソースは 403 ではなく 404」に揃える）。
pub async fn account_exists(
    db: &PgPool,
    user_id: Uuid,
    account_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let found = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM accounts WHERE id = $1 AND user_id = $2
        ) AS "exists!"
        "#,
        account_id,
        user_id,
    )
    .fetch_one(db)
    .await?;

    Ok(found)
}
