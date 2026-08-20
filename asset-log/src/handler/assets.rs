use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::asset::{Asset, AssetClass};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::repository::asset_repo::{self, AssetPatch, NewAsset, escape_like};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct AssetResponse {
    id: Uuid,
    symbol: String,
    name: String,
    asset_class: AssetClass,
    currency: String,
    #[serde(with = "rust_decimal::serde::str")]
    price_unit: Decimal,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<Asset> for AssetResponse {
    fn from(a: Asset) -> Self {
        Self {
            id: a.id,
            symbol: a.symbol,
            name: a.name,
            asset_class: a.asset_class,
            currency: a.currency,
            price_unit: a.price_unit,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAssetRequest {
    symbol: String,
    name: String,
    asset_class: AssetClass,
    #[serde(default = "default_currency")]
    currency: String,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    price_unit: Option<Decimal>,
}

fn default_currency() -> String {
    "JPY".to_string()
}

/// 通貨コードを正規化する。`usd` を422にせず `USD` として受ける。
fn normalize_currency(input: &str) -> Result<String, AppError> {
    let c = input.trim().to_uppercase();
    if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_uppercase()) {
        Ok(c)
    } else {
        Err(AppError::unprocessable("currency must be a 3-letter code"))
    }
}

pub async fn create_asset(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateAssetRequest>,
) -> Result<(StatusCode, Json<AssetResponse>), AppError> {
    let symbol = req.symbol.trim().to_string();
    let name = req.name.trim().to_string();
    if symbol.is_empty() || name.is_empty() {
        return Err(AppError::unprocessable("symbol and name must not be blank"));
    }

    let price_unit = req
        .price_unit
        .unwrap_or_else(|| req.asset_class.default_price_unit());
    if price_unit <= Decimal::ZERO {
        return Err(AppError::unprocessable("price_unit must be positive"));
    }

    let input = NewAsset {
        symbol,
        name,
        asset_class: req.asset_class,
        currency: normalize_currency(&req.currency)?,
        price_unit,
    };

    let asset = asset_repo::create(&state.db, user.0, input).await?;
    Ok((StatusCode::CREATED, Json(asset.into())))
}

#[derive(Debug, Deserialize)]
pub struct ListAssetsQuery {
    q: Option<String>,
}

pub async fn list_assets(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ListAssetsQuery>,
) -> Result<Json<Vec<AssetResponse>>, AppError> {
    let q = query.q.as_deref().map(escape_like);
    let assets = asset_repo::list(&state.db, user.0, q.as_deref()).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}

pub async fn get_asset(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<AssetResponse>, AppError> {
    asset_repo::find(&state.db, user.0, id)
        .await?
        .map(|a| Json(a.into()))
        .ok_or(AppError::NotFound("asset not found"))
}

#[derive(Debug, Deserialize)]
pub struct PatchAssetRequest {
    symbol: Option<String>,
    name: Option<String>,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    price_unit: Option<Decimal>,
}

pub async fn patch_asset(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchAssetRequest>,
) -> Result<Json<AssetResponse>, AppError> {
    if req.symbol.is_none() && req.name.is_none() && req.price_unit.is_none() {
        return Err(AppError::BadRequest("no fields to update".to_string()));
    }

    let symbol = match req.symbol {
        Some(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                return Err(AppError::field("symbol", "must not be blank"));
            }
            Some(s)
        }
        None => None,
    };

    let name = match req.name {
        Some(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                return Err(AppError::field("name", "must not be blank"));
            }
            Some(s)
        }
        None => None,
    };

    if let Some(u) = req.price_unit
        && u <= Decimal::ZERO
    {
        return Err(AppError::field("price_unit", "must be positive"));
    }

    let patch = AssetPatch {
        symbol,
        name,
        price_unit: req.price_unit,
    };

    asset_repo::update(&state.db, user.0, id, patch)
        .await?
        .map(|a| Json(a.into()))
        .ok_or(AppError::NotFound("asset not found"))
}
