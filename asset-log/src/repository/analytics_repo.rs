use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::AccountType;
use crate::domain::asset::AssetClass;
use crate::domain::position::TradeKind;

/// 畳み込みに必要な取引 + 口座・銘柄メタ
#[derive(Debug)]
pub struct HistoryTrade {
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub account_name: String,
    pub account_type: AccountType,
    pub symbol: String,
    pub asset_name: String,
    pub asset_class: AssetClass,
    pub currency: String,
    pub price_unit: Decimal,
    pub kind: TradeKind,
    pub quantity: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
    pub traded_at: NaiveDate,
}

/// 日付 × 銘柄の価格グリッド（価格未登録なら price が None）
#[derive(Debug)]
pub struct PricePoint {
    pub on_date: NaiveDate,
    pub asset_id: Uuid,
    pub price: Option<Decimal>,
    pub priced_on: Option<NaiveDate>,
}

pub async fn fetch_trades_until(
    db: &PgPool,
    user_id: Uuid,
    to: NaiveDate,
) -> Result<Vec<HistoryTrade>, sqlx::Error> {
    sqlx::query_as!(
        HistoryTrade,
        r#"
        SELECT
            t.account_id,
            t.asset_id,
            a.name         AS account_name,
            a.account_type AS "account_type: AccountType",
            s.symbol,
            s.name         AS asset_name,
            s.asset_class  AS "asset_class: AssetClass",
            s.currency,
            s.price_unit,
            t.kind         AS "kind: TradeKind",
            t.quantity,
            t.price,
            t.fee,
            t.traded_at
        FROM transactions t
        JOIN accounts a ON a.id = t.account_id
        JOIN assets   s ON s.id = t.asset_id
        WHERE t.user_id = $1 AND t.traded_at <= $2
        ORDER BY t.traded_at, t.created_at, t.id
        "#,
        user_id,
        to,
    )
    .fetch_all(db)
    .await
}

pub async fn fetch_price_grid(
    db: &PgPool,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    granularity: &str, // "day" | "month"
) -> Result<Vec<PricePoint>, sqlx::Error> {
    sqlx::query_as!(
        PricePoint,
        r#"
        WITH spine AS (
            SELECT d::date AS on_date
            FROM generate_series($2::date, $3::date, interval '1 day') AS d
            WHERE $4::text = 'day'
               OR d::date = $2::date
               OR d::date = $3::date
               OR d::date = (date_trunc('month', d) + interval '1 month - 1 day')::date
        ),
        held AS (
            SELECT DISTINCT asset_id
            FROM transactions
            WHERE user_id = $1 AND traded_at <= $3
        )
        SELECT
            sp.on_date    AS "on_date!",
            h.asset_id    AS "asset_id!",
            p.price       AS "price?",
            p.priced_on   AS "priced_on?"
        FROM spine sp
        CROSS JOIN held h
        LEFT JOIN LATERAL (
            SELECT price, priced_on
            FROM asset_prices
            WHERE asset_id = h.asset_id AND priced_on <= sp.on_date
            ORDER BY priced_on DESC
            LIMIT 1
        ) p ON true
        ORDER BY sp.on_date, h.asset_id
        "#,
        user_id,
        from,
        to,
        granularity,
    )
    .fetch_all(db)
    .await
}
