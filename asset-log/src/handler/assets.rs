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

use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, ToSchema)]
pub struct AssetResponse {
    pub id: Uuid,
    /// ティッカーや証券コード
    #[schema(example = "7203")]
    pub symbol: String,
    #[schema(example = "トヨタ自動車")]
    pub name: String,
    pub asset_class: AssetClass,
    #[schema(example = "JPY")]
    pub currency: String,
    /// 価格の単位（投信は10000、それ以外は1）。文字列で表現される
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "1")]
    pub price_unit: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAssetRequest {
    pub symbol: String,
    pub name: String,
    pub asset_class: AssetClass,
    /// ISO 4217 の3文字。小文字で送っても大文字に正規化される
    #[serde(default = "default_currency")]
    #[schema(example = "JPY")]
    pub currency: String,
    /// 未指定なら資産クラスの既定値（投信は10000、他は1）
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>, example = "10000")]
    pub price_unit: Option<Decimal>,
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
        Err(AppError::unprocessable(
            "通貨コードは3文字で指定してください",
        ))
    }
}
#[utoipa::path(
    post, path = "/assets", operation_id = "create_asset", tag = "assets",
    security(("bearerAuth" = [])),
    request_body = CreateAssetRequest,
    responses(
        (status = 201, description = "銘柄を作成した", body = AssetResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 409, description = "このシンボルは既に登録されている", body = ProblemDetails),
        (status = 422, description = "シンボル・名称が空、通貨コードの形式不正、価格単位が0以下", body = ProblemDetails)
    )
)]
pub async fn create_asset(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateAssetRequest>,
) -> Result<(StatusCode, Json<AssetResponse>), AppError> {
    let symbol = req.symbol.trim().to_string();
    let name = req.name.trim().to_string();
    if symbol.is_empty() || name.is_empty() {
        return Err(AppError::unprocessable("コードと名称は必須です"));
    }

    let price_unit = req
        .price_unit
        .unwrap_or_else(|| req.asset_class.default_price_unit());
    if price_unit <= Decimal::ZERO {
        return Err(AppError::unprocessable(
            "価格単位は正の数で指定してください",
        ));
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

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListAssetsQuery {
    /// シンボルまたは名称の部分一致検索
    pub q: Option<String>,
}
#[utoipa::path(
    get, path = "/assets", operation_id = "list_assets", tag = "assets",
    security(("bearerAuth" = [])),
    params(ListAssetsQuery),
    responses(
        (status = 200, description = "銘柄の一覧", body = Vec<AssetResponse>),
        (status = 401, description = "認証が必要", body = ProblemDetails)
    )
)]
pub async fn list_assets(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ListAssetsQuery>,
) -> Result<Json<Vec<AssetResponse>>, AppError> {
    let q = query.q.as_deref().map(escape_like);
    let assets = asset_repo::list(&state.db, user.0, q.as_deref()).await?;
    Ok(Json(assets.into_iter().map(Into::into).collect()))
}
#[utoipa::path(
    get, path = "/assets/{id}", operation_id = "get_asset", tag = "assets",
    security(("bearerAuth" = [])),
    params(("id" = Uuid, Path, description = "銘柄ID")),
    responses(
        (status = 200, description = "銘柄の詳細", body = AssetResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 404, description = "銘柄が存在しない", body = ProblemDetails)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchAssetRequest {
    pub symbol: Option<String>,
    pub name: Option<String>,
    #[serde(default, with = "rust_decimal::serde::str_option")]
    #[schema(value_type = Option<String>)]
    pub price_unit: Option<Decimal>,
}
#[utoipa::path(
    patch, path = "/assets/{id}", operation_id = "update_asset", tag = "assets",
    security(("bearerAuth" = [])),
    params(("id" = Uuid, Path, description = "銘柄ID")),
    request_body = PatchAssetRequest,
    responses(
        (status = 200, description = "更新後の銘柄", body = AssetResponse),
        (status = 400, description = "更新する項目が指定されていない", body = ProblemDetails),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 404, description = "銘柄が存在しない", body = ProblemDetails),
        (status = 409, description = "シンボルが他の銘柄と重複", body = ProblemDetails),
        (status = 422, description = "シンボル・名称が空、価格単位が0以下", body = ProblemDetails)
    )
)]
pub async fn patch_asset(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchAssetRequest>,
) -> Result<Json<AssetResponse>, AppError> {
    if req.symbol.is_none() && req.name.is_none() && req.price_unit.is_none() {
        return Err(AppError::BadRequest("更新する項目がありません".to_string()));
    }

    let symbol = match req.symbol {
        Some(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                return Err(AppError::field("symbol", "必須項目です"));
            }
            Some(s)
        }
        None => None,
    };

    let name = match req.name {
        Some(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                return Err(AppError::field("name", "必須項目です"));
            }
            Some(s)
        }
        None => None,
    };

    if let Some(u) = req.price_unit
        && u <= Decimal::ZERO
    {
        return Err(AppError::field("price_unit", "正の数を指定してください"));
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
