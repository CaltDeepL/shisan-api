use axum::Json;
use axum::extract::State;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::error::AppError;
use crate::middleware::auth::JobAuth;
use crate::service::snapshot_service::{self, RunReport};
use crate::state::AppState;

#[derive(Debug, Default, serde::Deserialize)]
pub struct RunRequest {
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    /// 指定するとそのユーザーだけを対象にする。分割実行・再計算用
    #[serde(default)]
    pub user_id: Option<Uuid>,
}

/// バッチ実行。認証は JobAuth（ユーザーJWTではない）。
pub async fn run(
    _: JobAuth,
    State(state): State<AppState>,
    body: Option<Json<RunRequest>>,
) -> Result<Json<RunReport>, AppError> {
    let req = body.map(|Json(b)| b).unwrap_or_default();

    if let (Some(from), Some(to)) = (req.from, req.to)
        && from > to
    {
        return Err(AppError::BadRequest("from は to 以前にしてください".into()));
    }

    let report =
        snapshot_service::run(&state.db, state.fx.as_ref(), req.from, req.to, req.user_id).await?;

    Ok(Json(report))
}
