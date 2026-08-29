use axum::http::{HeaderValue, Method, header};
use std::time::Duration;
use tower_http::cors::CorsLayer;

/// 許可オリジンから CORS レイヤを組み立てる。
/// 認証は Bearer トークンのため allow_credentials は不要。
pub fn cors_layer(origins: &[String]) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!(origin = %o, "CORS オリジンをヘッダ値に変換できません（スキップ）");
                None
            }
        })
        .collect();

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(600))
}
