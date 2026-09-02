use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

/// ヘルスチェックのレスポンス
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    /// 常に "ok"
    pub status: String,
}

#[utoipa::path(
    get,
    path = "/health",
    operation_id = "health",
    tag = "health",
    responses(
        (status = 200, description = "サービス稼働中", body = HealthResponse)
    )
)]
pub async fn health() -> Json<HealthResponse> {
    // 既存の中身のまま
    Json(HealthResponse {
        status: "ok".into(),
    })
}
