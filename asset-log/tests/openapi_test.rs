//! OpenAPI仕様が正しく生成・配信されていることを確認する。
//!
//! ルートを追加したのに `#[utoipa::path]` を書き忘れた、
//! `routes!()` に登録し忘れた、といった漏れをここで拾う。

mod common;

use axum::http::{Method, StatusCode};
use common::{request, test_app};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashSet;

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

const PROBLEM_DETAILS_REF: &str = "#/components/schemas/ProblemDetails";

/// ProblemDetails 以外を返すことを意図しているエラー応答。
/// CSV 本登録の 422 は、行番号や取込件数を返す ImportReport が API 契約そのもの。
const ERROR_RESPONSE_EXCEPTIONS: &[(&str, &str, &str, &str)] = &[(
    "/import/transactions",
    "post",
    "422",
    "#/components/schemas/ImportReport",
)];

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
async fn documented_error_responses_use_problem_details(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    let paths = spec["paths"].as_object().expect("paths");
    let methods = [
        "get", "post", "put", "patch", "delete", "options", "head", "trace",
    ];
    let mut seen_exceptions = HashSet::new();

    for (path, item) in paths {
        for method in methods {
            let Some(operation) = item.get(method) else {
                continue;
            };

            let responses = operation["responses"]
                .as_object()
                .unwrap_or_else(|| panic!("{method} {path} に responses がありません"));

            for (status, response) in responses {
                if !status.starts_with('4') && !status.starts_with('5') {
                    continue;
                }

                let exception = ERROR_RESPONSE_EXCEPTIONS.iter().find(
                    |(exception_path, exception_method, exception_status, _)| {
                        path.as_str() == *exception_path
                            && method == *exception_method
                            && status.as_str() == *exception_status
                    },
                );

                let expected_ref = if let Some((_, _, _, schema_ref)) = exception {
                    seen_exceptions.insert(format!("{method} {path} {status}"));
                    *schema_ref
                } else {
                    PROBLEM_DETAILS_REF
                };

                let content = response["content"]
                    .as_object()
                    .unwrap_or_else(|| panic!("{method} {path} {status} に content がありません"));

                let refs: Vec<&str> = content
                    .values()
                    .filter_map(|media| media["schema"]["$ref"].as_str())
                    .collect();

                assert!(
                    refs.contains(&expected_ref),
                    "{method} {path} {status} の schema が想定外です: {refs:?}; expected {expected_ref}"
                );
            }
        }
    }

    for (path, method, status, _) in ERROR_RESPONSE_EXCEPTIONS {
        let key = format!("{method} {path} {status}");
        assert!(
            seen_exceptions.contains(&key),
            "例外指定 {key} が spec に存在しません。不要になった例外は削除してください"
        );
    }
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

/// operation_id の付け忘れ・重複を検知する。
/// utoipa は明示しないと Rust の関数名をそのまま operationId にするため、
/// 別ハンドラで同じ関数名（list / create など）を使うと黙って衝突する。
#[sqlx::test]
async fn operation_ids_are_unique(db: PgPool) {
    let app = test_app(db);
    let (_, spec) = request(&app, Method::GET, "/openapi.json", None, None).await;

    let paths = spec["paths"].as_object().expect("paths");
    let mut ids = Vec::new();

    for (path, item) in paths {
        for (method, op) in item.as_object().expect("path item") {
            let id = op["operationId"]
                .as_str()
                .unwrap_or_else(|| panic!("{method} {path} に operation_id がありません"));
            ids.push(format!("{id} ({method} {path})"));
        }
    }

    let mut names: Vec<&str> = ids
        .iter()
        .map(|s| s.split(' ').next().expect("id"))
        .collect();
    let total = names.len();
    names.sort();
    names.dedup();

    assert_eq!(names.len(), total, "operation_id が重複しています: {ids:?}");
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
