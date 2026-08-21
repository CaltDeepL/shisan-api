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
use crate::service::holdings_service::{self, HoldingsQuery, HoldingsResponse};
use crate::state::AppState;

/// `GET /holdings?account_id=<uuid>&include_closed=<bool>`
///
/// - `account_id`: 省略時は全口座。他人の・存在しない口座は 404
/// - `include_closed`: 既定 `false`。`true` で全売却済み（数量0）のポジションも含める
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<HoldingsQuery>,
) -> Result<Json<HoldingsResponse>, AppError> {
    let response = holdings_service::list_holdings(&state.db, user.0, query).await?;

    Ok(Json(response))
}
