//! CSVによる取引の一括取込。
//!
//! dry-run は検証のみでDBに書き込まない。本登録は1トランザクションで全行を挿入し、
//! 検証エラーが1件でもあれば何も挿入せず422を返す。

use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::service::import_service::{ImportReport, ImportResult};
use crate::{
    error::AppError, middleware::auth::AuthUser, service::import_service, state::AppState,
};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportRequest {
    /// CSV本文。ヘッダ行を含む
    #[schema(example = "account_id,asset_id,kind,quantity,price,fee,traded_at\n...")]
    pub csv_content: String,
}
#[utoipa::path(
    post, path = "/import/transactions/dry-run", tag = "import",
    security(("bearerAuth" = [])),
    request_body = ImportRequest,
    responses(
        (status = 200, description = "検証結果。DBには何も書き込まれない。errors が空でなければ本登録は失敗する", body = ImportReport),
        (status = 401, description = "認証が必要", body = ProblemDetails)
    )
)]
pub async fn dry_run(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<ImportRequest>,
) -> Result<Json<import_service::ImportReport>, AppError> {
    let report = import_service::dry_run_report(&state.db, user.0, &payload.csv_content).await?;
    Ok(Json(report))
}
#[utoipa::path(
    post, path = "/import/transactions", tag = "import",
    security(("bearerAuth" = [])),
    request_body = ImportRequest,
    responses(
        (status = 200, description = "全行の取込に成功した", body = ImportResult),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 422, description = "検証エラー。1件でもあれば何も挿入されない。ボディは ProblemDetails ではなく ImportReport", body = ImportReport)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<ImportRequest>,
) -> Result<axum::response::Response, AppError> {
    let outcome = import_service::import(&state.db, user.0, &payload.csv_content).await?;
    Ok(match outcome {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(report) => (StatusCode::UNPROCESSABLE_ENTITY, Json(report)).into_response(),
    })
}
