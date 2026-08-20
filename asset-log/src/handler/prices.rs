use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::asset::AssetPrice;
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::repository::{
    asset_repo,
    price_repo::{self, PriceInput},
};
use crate::state::AppState;

const MAX_PRICES_PER_REQUEST: usize = 1000;

#[derive(Debug, Deserialize)]
pub struct UpsertPricesRequest {
    asset_id: Uuid,
    #[serde(default)]
    source: Option<String>,
    prices: Vec<PriceItem>,
}

#[derive(Debug, Deserialize)]
pub struct PriceItem {
    priced_on: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    price: Decimal,
}

#[derive(Debug, Serialize)]
pub struct UpsertPricesResponse {
    upserted: u64,
}

#[derive(Debug, Deserialize)]
pub struct PriceHistoryQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct PriceResponse {
    priced_on: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    price: Decimal,
    source: String,
    updated_at: DateTime<Utc>,
}

impl From<AssetPrice> for PriceResponse {
    fn from(p: AssetPrice) -> Self {
        // asset_id はパスに含まれるため返さない
        Self {
            priced_on: p.priced_on,
            price: p.price,
            source: p.source,
            updated_at: p.updated_at,
        }
    }
}

pub async fn upsert_prices(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpsertPricesRequest>,
) -> Result<Json<UpsertPricesResponse>, AppError> {
    // 空配列を先に弾く。後回しにすると rows_affected == 0 が
    // 「銘柄が存在しない」と区別できなくなる。
    if req.prices.is_empty() {
        return Err(AppError::BadRequest("prices must not be empty".to_string()));
    }
    if req.prices.len() > MAX_PRICES_PER_REQUEST {
        return Err(AppError::unprocessable("too many prices in one request"));
    }

    let today = Utc::now().date_naive();
    for p in &req.prices {
        if p.priced_on > today {
            return Err(AppError::field("priced_on", "must not be in the future"));
        }
        if p.price < Decimal::ZERO {
            return Err(AppError::field("price", "must not be negative"));
        }
    }

    let rows: Vec<PriceInput> = req
        .prices
        .iter()
        .map(|p| PriceInput {
            priced_on: p.priced_on,
            price: p.price,
        })
        .collect();
    let source = req.source.as_deref().unwrap_or("manual");

    let upserted = price_repo::upsert_many(&state.db, user.0, req.asset_id, &rows, source).await?;

    if upserted == 0 {
        return Err(AppError::NotFound("asset not found"));
    }

    Ok(Json(UpsertPricesResponse { upserted }))
}

pub async fn get_price_history(
    State(state): State<AppState>,
    user: AuthUser,
    Path(asset_id): Path<Uuid>,
    Query(q): Query<PriceHistoryQuery>,
) -> Result<Json<Vec<PriceResponse>>, AppError> {
    // 空配列が「銘柄なし」か「価格未登録」かを区別するため先に確認する
    if asset_repo::find(&state.db, user.0, asset_id)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound("asset not found"));
    }

    let prices = price_repo::history(&state.db, user.0, asset_id, q.from, q.to).await?;
    Ok(Json(prices.into_iter().map(Into::into).collect()))
}
