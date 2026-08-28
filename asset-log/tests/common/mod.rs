#![allow(dead_code)]

use asset_log::{
    auth::{job::JobToken, jwt::JwtKeys},
    provider::{
        cached_fx::{CachedFxProvider, StalePolicy},
        fx::FrankfurterClient,
    },
    state::AppState,
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use std::{sync::Arc, time::Duration};

/// テスト用のルータを組む。JWT の鍵はテスト内で固定。
/// 外部APIは到達不能なURLを指すので、fx を叩かないテストはこれで足りる。
pub fn test_app(db: PgPool) -> Router {
    test_app_with_fx(db, "http://127.0.0.1:1/unreachable", StalePolicy::default())
}

/// 為替のテスト用。`fx_base_url` に wiremock のURLを渡す。
#[allow(dead_code)]
pub fn test_app_with_fx(db: PgPool, fx_base_url: &str, policy: StalePolicy) -> Router {
    let jwt = JwtKeys::new("test-secret-for-integration-tests", 60);

    let client = FrankfurterClient::new(fx_base_url, Duration::from_millis(500))
        .expect("failed to build FX client");
    let fx = Arc::new(CachedFxProvider::new(client, db.clone(), policy));

    asset_log::app(AppState {
        db,
        jwt,
        fx,
        job_token: JobToken::disabled(),
    })
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
    let token = json["access_token"]
        .as_str()
        .expect("access_token")
        .to_owned();

    let (_, me) = request(app, Method::GET, "/me", Some(&token), None).await;
    let id = me["user_id"]
        .as_str()
        .expect("user_id")
        .parse()
        .expect("uuid");

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

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, json)
}
/// バッチ用トークン。snapshots_test が Authorization ヘッダに使う。
#[allow(dead_code)]
pub const JOB_TOKEN: &str = "test-job-token-0123456789abcdefghij";

/// バッチ実行を有効にしたルータ。
#[allow(dead_code)]
pub fn test_app_with_job_token(db: PgPool) -> Router {
    let jwt = JwtKeys::new("test-secret-for-integration-tests", 60);
    let client =
        FrankfurterClient::new("http://127.0.0.1:1/unreachable", Duration::from_millis(500))
            .expect("failed to build FX client");
    let fx = Arc::new(CachedFxProvider::new(
        client,
        db.clone(),
        StalePolicy::default(),
    ));

    asset_log::app(AppState {
        db,
        jwt,
        fx,
        job_token: JobToken::new(JOB_TOKEN),
    })
}
