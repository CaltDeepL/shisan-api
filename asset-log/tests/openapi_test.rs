//! OpenAPI仕様が正しく生成・配信されていることを確認する。
//!
//! ルートを追加したのに `#[utoipa::path]` を書き忘れた、
//! `routes!()` に登録し忘れた、といった漏れをここで拾う。

mod common;

use axum::http::{Method, StatusCode};
use common::{request, test_app};
use serde_json::Value;
use sqlx::PgPool;

/// 現在公開しているパス。エンドポイントを増やしたらここも増やす。
const EXPECTED_PATHS: &[&str] = &[
    "/accounts",
    "/accounts/{id}",
    "/analytics/allocation",
    "/analytics/asset-history",
    "/assets",
    "/assets/{id}",
    "/auth/login",
    "/auth/register",
    "/fx/rates",
    "/health",
    "/holdings",
    "/import/transactions",
    "/import/transactions/dry-run",
    "/me",
    "/prices",
    "/prices/{asset_id}",
    "/snapshots/run",
    "/transactions",
    "/transactions/{id}",
];

/// spec を docs/openapi.json に書き出す。
/// 差分がPRに出るので、APIの契約変更が目に見える。
#[sqlx::test]
async fn spec_is_written_to_docs(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    let pretty = serde_json::to_string_pretty(&spec).expect("serialize");
    let path = std::path::Path::new("docs/openapi.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, pretty).expect("write");
}

#[sqlx::test]
async fn all_routes_are_documented(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    let paths = spec["paths"].as_object().expect("paths");
    for expected in EXPECTED_PATHS {
        assert!(paths.contains_key(*expected), "{expected} が spec にない");
    }
    assert_eq!(
        paths.len(),
        EXPECTED_PATHS.len(),
        "パス数が想定と違う。エンドポイントを増やしたら EXPECTED_PATHS も更新すること"
    );
}

#[sqlx::test]
async fn security_schemes_are_defined(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    let schemes = spec["components"]["securitySchemes"]
        .as_object()
        .expect("securitySchemes");
    assert!(schemes.contains_key("bearerAuth"));
    assert!(schemes.contains_key("jobToken"));

    // バッチ実行はユーザーJWTではなく jobToken を要求する
    let security = &spec["paths"]["/snapshots/run"]["post"]["security"];
    assert_eq!(security[0]["jobToken"], Value::Array(vec![]));
}

#[sqlx::test]
async fn error_schema_matches_problem_details(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    let problem = &spec["components"]["schemas"]["ProblemDetails"];
    let props = problem["properties"].as_object().expect("properties");

    // serde の rename が効いているか（kind ではなく type）
    assert!(props.contains_key("type"));
    assert!(props.contains_key("trace_id"));
    assert!(!props.contains_key("kind"));

    // errors は空なら省略されるので必須ではない
    let required: Vec<&str> = problem["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert!(!required.contains(&"errors"));
}

#[sqlx::test]
async fn decimal_fields_are_strings(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    // Decimal は文字列でシリアライズされる。spec もそれに合っていること
    let schemas = &spec["components"]["schemas"];
    assert_eq!(
        schemas["TransactionResponse"]["properties"]["quantity"]["type"],
        "string"
    );
    assert_eq!(
        schemas["AssetResponse"]["properties"]["price_unit"]["type"],
        "string"
    );
    assert_eq!(
        schemas["HoldingItem"]["properties"]["book_value"]["type"],
        "string"
    );
}

#[sqlx::test]
async fn swagger_ui_is_served(db: PgPool) {
    let app = test_app(db);
    let (status, _) = request(&app, Method::GET, "/docs", None, None).await;

    // SwaggerUi はスラッシュ有無でリダイレクトすることがある
    assert!(
        status == StatusCode::OK || status.is_redirection(),
        "unexpected status: {status}"
    );
}
