use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("internal server error")]
    Internal(#[from] anyhow::Error),

    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("not found")]
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, title) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not Found"),
            AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
            }
        };

        // 内部エラーの詳細はログにだけ出し、レスポンスには漏らさない
        if matches!(self, AppError::Database(_) | AppError::Internal(_)) {
            tracing::error!(error = ?self, "request failed");
        }

        let body = Json(json!({
            "type": "about:blank",
            "title": title,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}