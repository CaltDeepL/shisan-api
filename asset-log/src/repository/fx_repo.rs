use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::domain::currency::Currency;

pub struct CachedRate {
    pub rated_on: NaiveDate,
    pub rate: Decimal,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 期間の被覆状況。3列とも集約なので全部 nullable。
pub struct Coverage {
    pub seed_on: Option<NaiveDate>,
    pub newest_on: Option<NaiveDate>,
    pub max_gap: Option<i32>,
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

pub async fn coverage(
    pool: &PgPool,
    base: Currency,
    quote: Currency,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Coverage, sqlx::Error> {
    sqlx::query_as!(
        Coverage,
        r#"
        WITH r AS (
            SELECT rated_on FROM fx_rates
            WHERE base = $1 AND quote = $2 AND rated_on BETWEEN $3 AND $4
        ),
        g AS (
            SELECT rated_on - lag(rated_on) OVER (ORDER BY rated_on) AS gap FROM r
        )
        SELECT
            (SELECT max(rated_on) FROM fx_rates
              WHERE base = $1 AND quote = $2 AND rated_on <= $3) AS "seed_on?",
            (SELECT max(rated_on) FROM r) AS "newest_on?",
            (SELECT max(gap) FROM g)      AS "max_gap?"
        "#,
        base.as_str(),
        quote.as_str(),
        from,
        to,
    )
    .fetch_one(pool)
    .await
}

/// 期間内のレートを昇順で全件。
pub async fn find_in_range(
    pool: &PgPool,
    base: Currency,
    quote: Currency,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<CachedRate>, sqlx::Error> {
    sqlx::query_as!(
        CachedRate,
        r#"
        SELECT rated_on, rate, fetched_at
        FROM fx_rates
        WHERE base = $1 AND quote = $2 AND rated_on BETWEEN $3 AND $4
        ORDER BY rated_on
        "#,
        base.as_str(),
        quote.as_str(),
        from,
        to,
    )
    .fetch_all(pool)
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

/// UNNEST による一括 UPSERT。1リクエストで250件入ることを想定。
pub async fn upsert_many(
    pool: &PgPool,
    base: Currency,
    quote: Currency,
    points: &[(NaiveDate, Decimal)],
) -> Result<(), sqlx::Error> {
    if points.is_empty() {
        return Ok(());
    }
    let days: Vec<NaiveDate> = points.iter().map(|p| p.0).collect();
    let rates: Vec<Decimal> = points.iter().map(|p| p.1).collect();

    sqlx::query!(
        r#"
        INSERT INTO fx_rates (base, quote, rated_on, rate)
        SELECT $1::char(3), $2::char(3), t.d, t.r
        FROM unnest($3::date[], $4::numeric[]) AS t(d, r)
        ON CONFLICT (base, quote, rated_on)
        DO UPDATE SET rate = EXCLUDED.rate, fetched_at = now()
        "#,
        base.as_str(),
        quote.as_str(),
        &days,
        &rates,
    )
    .execute(pool)
    .await?;
    Ok(())
}
