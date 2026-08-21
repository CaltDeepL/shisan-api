use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    domain::currency::Currency, error::AppError, middleware::auth::AuthUser, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct FxQuery {
    pub base: String,
    pub quote: String,
    /// 省略時は今日
    pub on: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct FxRateResponse {
    pub base: String,
    pub quote: String,
    /// 実際にレートが成立した日。要求した `on` と一致するとは限らない
    pub rated_on: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    pub rate: Decimal,
    /// 外部APIに到達できずキャッシュで代替した場合 true
    pub is_stale: bool,
    pub fetched_at: chrono::DateTime<Utc>,
}

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
