//! `GET /holdings`
//!
//! 保有ポジション一覧を、現在価格を当てた評価損益つきで返す。
//! 集計ロジックはすべて `service::holdings_service` にあり、ここは入出力の変換だけ。

use axum::{
    Json,
    extract::{Query, State},
};

use crate::error::AppError;
// NOTE: AuthUser のインポートパスは handler/transactions.rs からコピーして合わせること。
use crate::middleware::auth::AuthUser;
use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::service::holdings_service::{self, HoldingsQuery, HoldingsResponse};
use crate::state::AppState;

/// `GET /holdings?account_id=<uuid>&include_closed=<bool>`
///
/// - `account_id`: 省略時は全口座。他人の・存在しない口座は 404
/// - `include_closed`: 既定 `false`。`true` で全売却済み（数量0）のポジションも含める
/// - `include_unpriced`: 既定 `false`。`true` で価格が無く評価対象外になったポジションも含める
/// - `include_zero`: 既定 `false`。`true` で数量0のポジションも含める

#[utoipa::path(
    get, path = "/holdings", tag = "holdings",
    security(("bearerAuth" = [])),
    params(HoldingsQuery),
    responses(
        (status = 200, description = "保有ポジションと通貨別・口座別の集計", body = HoldingsResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 404, description = "指定した口座が存在しない", body = ProblemDetails)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<HoldingsQuery>,
) -> Result<Json<HoldingsResponse>, AppError> {
    let response = holdings_service::list_holdings(&state.db, user.0, query).await?;

    Ok(Json(response))
}
