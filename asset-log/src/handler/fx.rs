use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::{
    domain::currency::Currency, error::AppError, middleware::auth::AuthUser, state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, IntoParams)]
pub struct FxQuery {
    /// 変換元の通貨コード（3文字）
    #[param(example = "USD")]
    pub base: String,
    /// 変換先の通貨コード（3文字）
    #[param(example = "JPY")]
    pub quote: String,
    /// 省略時は今日
    pub on: Option<NaiveDate>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FxRateResponse {
    pub base: String,
    pub quote: String,
    /// 実際にレートが成立した日。要求した `on` と一致するとは限らない
    pub rated_on: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String, example = "157.23")]
    pub rate: Decimal,
    /// 外部APIに到達できずキャッシュで代替した場合 true
    pub is_stale: bool,
    pub fetched_at: chrono::DateTime<Utc>,
}

#[utoipa::path(
    get, path = "/fx/rates", operation_id = "get_rate", tag = "fx",
    security(("bearerAuth" = [])),
    params(FxQuery),
    responses(
        (status = 200, description = "為替レート。外部APIに到達できない場合はキャッシュを返し is_stale=true になる", body = FxRateResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 422, description = "通貨コードの形式不正、未来日、対応していない通貨ペア", body = ProblemDetails),
        (status = 503, description = "外部APIに到達できず、キャッシュも古すぎる", body = ProblemDetails)
    )
)]
pub async fn get_rate(
    _user: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<FxQuery>,
) -> Result<Json<FxRateResponse>, AppError> {
    let base: Currency = q
        .base
        .parse()
        .map_err(|_| AppError::field("base", "3文字の通貨コードで指定してください"))?;
    let quote: Currency = q
        .quote
        .parse()
        .map_err(|_| AppError::field("quote", "3文字の通貨コードで指定してください"))?;

    let today = Utc::now().date_naive();
    let on = q.on.unwrap_or(today);
    if on > today {
        return Err(AppError::field("on", "未来日は指定できません"));
    }
    let r = state.fx.rate(base, quote, on).await?;

    Ok(Json(FxRateResponse {
        base: r.base.to_string(),
        quote: r.quote.to_string(),
        rated_on: r.rated_on,
        rate: r.rate.normalize(),
        is_stale: r.is_stale,
        fetched_at: r.fetched_at,
    }))
}
