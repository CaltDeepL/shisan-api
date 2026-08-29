mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use tower::ServiceExt; // oneshot

const ALLOWED: &str = "http://localhost:5173";
const DISALLOWED: &str = "https://evil.example";

fn allowed_origins() -> Vec<String> {
    vec![ALLOWED.to_string()]
}

/// プリフライト（OPTIONS）に CORS ヘッダが返る
#[sqlx::test(migrations = "./migrations")]
async fn preflight_returns_cors_headers(db: sqlx::PgPool) {
    let app = common::test_app_with_cors(db, &allowed_origins());

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/accounts")
                .header(header::ORIGIN, ALLOWED)
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(
                    header::ACCESS_CONTROL_REQUEST_HEADERS,
                    "authorization,content-type",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let h = res.headers();
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], ALLOWED);

    let allow_headers = h[header::ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap()
        .to_lowercase();
    assert!(allow_headers.contains("authorization"));
    assert!(allow_headers.contains("content-type"));

    let allow_methods = h[header::ACCESS_CONTROL_ALLOW_METHODS].to_str().unwrap();
    assert!(allow_methods.contains("POST"));
}

/// 許可外オリジンにはヘッダが付かない（サーバー自体は正常応答する）
#[sqlx::test(migrations = "./migrations")]
async fn disallowed_origin_gets_no_cors_header(db: sqlx::PgPool) {
    let app = common::test_app_with_cors(db, &allowed_origins());

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .header(header::ORIGIN, DISALLOWED)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        !res.headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}

/// 単純リクエストにヘッダが付く
#[sqlx::test(migrations = "./migrations")]
async fn simple_request_gets_cors_header(db: sqlx::PgPool) {
    let app = common::test_app_with_cors(db, &allowed_origins());

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .header(header::ORIGIN, ALLOWED)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], ALLOWED);
}

/// 401 レスポンスにもヘッダが付く（レイヤが最外層にあることの検証）
#[sqlx::test(migrations = "./migrations")]
async fn unauthorized_response_has_cors_header(db: sqlx::PgPool) {
    let app = common::test_app_with_cors(db, &allowed_origins());

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/accounts")
                .header(header::ORIGIN, ALLOWED)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], ALLOWED);
}

/// 許可オリジンが空なら誰も通らない
#[sqlx::test(migrations = "./migrations")]
async fn empty_origin_list_allows_nobody(db: sqlx::PgPool) {
    let app = common::test_app_with_cors(db, &[]);

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .header(header::ORIGIN, ALLOWED)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        !res.headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}
