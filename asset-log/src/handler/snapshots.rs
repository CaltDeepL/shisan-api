use crate::error::AppError;
use crate::middleware::auth::JobAuth;
use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::service::snapshot_service::{self, RunReport};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use chrono::NaiveDate;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Default, serde::Deserialize, ToSchema)]
pub struct RunRequest {
    /// 開始日。既定は to の6日前
    #[serde(default)]
    pub from: Option<NaiveDate>,
    /// 終了日。既定は当日
    #[serde(default)]
    pub to: Option<NaiveDate>,
    /// 指定するとそのユーザーだけを対象にする。分割実行・再計算用
    #[serde(default)]
    pub user_id: Option<Uuid>,
}
#[utoipa::path(
    post, path = "/snapshots/run", operation_id = "run_snapshots", tag = "snapshots",
    security(("jobToken" = [])),
    request_body(content = RunRequest, description = "省略可。ボディなしなら直近7日分を全ユーザーで実行"),
    responses(
        (status = 200, description = "バッチ実行の結果", body = RunReport),
        (status = 400, description = "from が to より後", body = ProblemDetails),
        (status = 401, description = "バッチトークンが不正", body = ProblemDetails)
    )
)]
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
