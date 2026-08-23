//! CSVによる取引の一括取込。
//!
//! dry-run は検証のみでDBに書き込まない。本登録は1トランザクションで全行を挿入し、
//! 検証エラーが1件でもあれば何も挿入せず422を返す。

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;

use crate::{
    error::AppError, middleware::auth::AuthUser, service::import_service, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub csv_content: String,
}

pub async fn dry_run(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<ImportRequest>,
) -> Result<Json<import_service::ImportReport>, AppError> {
    let report = import_service::dry_run_report(&state.db, user.0, &payload.csv_content).await?;
    Ok(Json(report))
}

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
