use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::asset::AssetPrice;
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct PriceInput {
    pub priced_on: NaiveDate,
    pub price: Decimal,
}

/// 価格を一括登録する。同一 (asset_id, priced_on) は上書き。
/// 銘柄が存在しないか他人のものなら 0 を返す（handler が404にする）。
///
/// `rows` は空でないこと。空配列は handler 側で 400 にしてから呼ぶ。
pub async fn upsert_many(
    pool: &PgPool,
    user_id: Uuid,
    asset_id: Uuid,
    rows: &[PriceInput],
    source: &str,
) -> Result<u64, AppError> {
    // 同一日付が2件あると ON CONFLICT DO UPDATE が同じ行を二度更新できず
    // SQLSTATE 21000 で落ちるため、ここで後勝ちに正規化する。
    let mut dedup: BTreeMap<NaiveDate, Decimal> = BTreeMap::new();
    for row in rows {
        dedup.insert(row.priced_on, row.price);
    }
    let (priced_on, price): (Vec<NaiveDate>, Vec<Decimal>) = dedup.into_iter().unzip();

    let result = sqlx::query!(
        r#"
        INSERT INTO asset_prices (asset_id, priced_on, price, source)
        SELECT a.id, u.priced_on, u.price, $5
        FROM assets a
        CROSS JOIN UNNEST($3::date[], $4::numeric[]) AS u(priced_on, price)
        WHERE a.id = $1 AND a.user_id = $2
        ON CONFLICT (asset_id, priced_on)
        DO UPDATE SET price = EXCLUDED.price, source = EXCLUDED.source
        "#,
        asset_id,
        user_id,
        &priced_on[..],
        &price[..],
        source,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn history(
    pool: &PgPool,
    user_id: Uuid,
    asset_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<Vec<AssetPrice>, AppError> {
    let prices = sqlx::query_as!(
        AssetPrice,
        r#"
        SELECT p.asset_id, p.priced_on, p.price, p.source, p.updated_at
        FROM asset_prices p
        JOIN assets a ON a.id = p.asset_id
        WHERE p.asset_id = $1
          AND a.user_id  = $2
          AND ($3::date IS NULL OR p.priced_on >= $3)
          AND ($4::date IS NULL OR p.priced_on <= $4)
        ORDER BY p.priced_on DESC
        "#,
        asset_id,
        user_id,
        from,
        to,
    )
    .fetch_all(pool)
    .await?;

    Ok(prices)
}
