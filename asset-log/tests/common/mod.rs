use asset_log::{auth::jwt::JwtKeys, state::AppState};
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// テスト用のルータを組む。JWT の鍵はテスト内で固定
pub fn test_app(db: PgPool) -> Router {
    let jwt = JwtKeys::new("test-secret-for-integration-tests", 60);
    asset_log::app(AppState { db, jwt })
}

pub struct TestUser {
    #[allow(dead_code)]
    pub id: Uuid,
    pub token: String,
}

/// ユーザーを登録してトークンを取得する。
/// リポジトリ層ではなく API 経由にしているのは、
/// 認証まで含めた本番と同じ経路を通すため。
pub async fn register_user(app: &Router, email: &str) -> TestUser {
    let body = json!({ "email": email, "password": "password1234" });
    let (status, json) = request(app, Method::POST, "/auth/register", None, Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {json}");
    let token = json["access_token"].as_str().expect("access_token").to_owned();

    let (_, me) = request(app, Method::GET, "/me", Some(&token), None).await;
    let id = me["user_id"].as_str().expect("user_id").parse().expect("uuid");

    TestUser { id, token }
}

/// リクエストを1本投げて (ステータス, JSON) を返す。
/// ボディが空なら Value::Null
pub async fn request(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => builder.body(Body::empty()),
    }
    .expect("failed to build request");

    let response = app.clone().oneshot(request).await.expect("request failed");
    let status = response.status();

    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, json)
}