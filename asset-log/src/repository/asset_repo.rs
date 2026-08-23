use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::asset::{Asset, AssetClass};
use crate::error::AppError;

#[derive(Debug)]
pub struct NewAsset {
    pub symbol: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub currency: String,
    pub price_unit: Decimal,
}

#[derive(Debug, Default)]
pub struct AssetPatch {
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub price_unit: Option<Decimal>,
}

pub async fn create(pool: &PgPool, user_id: Uuid, input: NewAsset) -> Result<Asset, AppError> {
    let asset = sqlx::query_as!(
        Asset,
        r#"
        INSERT INTO assets (user_id, symbol, name, asset_class, currency, price_unit)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING
            id,
            user_id,
            symbol,
            name,
            asset_class AS "asset_class: AssetClass",
            currency,
            price_unit,
            created_at,
            updated_at
        "#,
        user_id,
        input.symbol,
        input.name,
        input.asset_class as AssetClass,
        input.currency,
        input.price_unit,
    )
    .fetch_one(pool)
    .await?;

    Ok(asset)
}
pub async fn list(pool: &PgPool, user_id: Uuid, q: Option<&str>) -> Result<Vec<Asset>, AppError> {
    let assets = sqlx::query_as!(
        Asset,
        r#"
        SELECT
            id,
            user_id,
            symbol,
            name,
            asset_class AS "asset_class: AssetClass",
            currency,
            price_unit,
            created_at,
            updated_at
        FROM assets
        WHERE user_id = $1
          AND (
                $2::text IS NULL
             OR symbol ILIKE '%' || $2 || '%'
             OR name   ILIKE '%' || $2 || '%'
          )
        ORDER BY upper(symbol)
        "#,
        user_id,
        q,
    )
    .fetch_all(pool)
    .await?;

    Ok(assets)
}

/// LIKE のメタ文字を無効化する。ESCAPE 句の既定は '\'。
pub(crate) fn escape_like(s: &str) -> String {
    // 💡 pub(crate) を付ける
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
pub async fn find(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<Asset>, AppError> {
    let asset = sqlx::query_as!(
        Asset,
        r#"
        SELECT
            id, user_id, symbol, name,
            asset_class AS "asset_class: AssetClass",
            currency, price_unit, created_at, updated_at
        FROM assets
        WHERE id = $1 AND user_id = $2
        "#,
        id,
        user_id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(asset)
}
pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    patch: AssetPatch,
) -> Result<Option<Asset>, AppError> {
    let asset = sqlx::query_as!(
        Asset,
        r#"
        UPDATE assets
        SET symbol     = COALESCE($3, symbol),
            name       = COALESCE($4, name),
            price_unit = COALESCE($5, price_unit)
        WHERE id = $1 AND user_id = $2
        RETURNING
            id, user_id, symbol, name,
            asset_class AS "asset_class: AssetClass",
            currency, price_unit, created_at, updated_at
        "#,
        id,
        user_id,
        patch.symbol,
        patch.name,
        patch.price_unit,
    )
    .fetch_optional(pool)
    .await?;

    Ok(asset)
}
/// symbol で1件引く。`assets_user_symbol_key` が upper(symbol) のため大文字小文字は無視する。
pub async fn find_by_symbol(
    pool: &PgPool,
    user_id: Uuid,
    symbol: &str,
) -> Result<Option<Asset>, AppError> {
    let asset = sqlx::query_as!(
        Asset,
        r#"
        SELECT
            id, user_id, symbol, name,
            asset_class AS "asset_class: AssetClass",
            currency, price_unit, created_at, updated_at
        FROM assets
        WHERE user_id = $1 AND upper(symbol) = upper($2)
        "#,
        user_id,
        symbol,
    )
    .fetch_optional(pool)
    .await?;

    Ok(asset)
}
