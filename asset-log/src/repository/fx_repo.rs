use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::domain::currency::Currency;

pub struct CachedRate {
    pub rated_on: NaiveDate,
    pub rate: Decimal,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 指定日以前で最も新しいレートを1件返す。
pub async fn find_latest_on_or_before(
    pool: &PgPool,
    base: Currency,
    quote: Currency,
    on: NaiveDate,
) -> Result<Option<CachedRate>, sqlx::Error> {
    sqlx::query_as!(
        CachedRate,
        r#"
        SELECT rated_on, rate, fetched_at
        FROM fx_rates
        WHERE base = $1 AND quote = $2 AND rated_on <= $3
        ORDER BY rated_on DESC
        LIMIT 1
        "#,
        base.as_str(),
        quote.as_str(),
        on,
    )
    .fetch_optional(pool)
    .await
}

pub async fn upsert(
    pool: &PgPool,
    base: Currency,
    quote: Currency,
    rated_on: NaiveDate,
    rate: Decimal,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO fx_rates (base, quote, rated_on, rate)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (base, quote, rated_on)
        DO UPDATE SET rate = EXCLUDED.rate, fetched_at = now()
        "#,
        base.as_str(),
        quote.as_str(),
        rated_on,
        rate,
    )
    .execute(pool)
    .await?;
    Ok(())
}
