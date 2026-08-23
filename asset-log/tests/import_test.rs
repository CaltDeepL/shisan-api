//! CSVインポートの統合テスト。

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use common::{register_user, request, test_app};

const HEADER: &str = "account,symbol,kind,quantity,price,fee,traded_at,note,external_id";

/// 取込先の口座と銘柄を用意する。
async fn setup_fixtures(app: &axum::Router, token: &str) {
    let (status, _) = request(
        app,
        Method::POST,
        "/accounts",
        Some(token),
        Some(json!({
            "name": "特定",
            "account_type": "tokutei",
            "institution": "証券会社",
            "currency": "JPY",
            "withholding": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request(
        app,
        Method::POST,
        "/assets",
        Some(token),
        Some(json!({
            "symbol": "7203",
            "name": "トヨタ自動車",
            "asset_class": "equity",
            "currency": "JPY",
            "price_unit": "1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

fn csv(rows: &[&str]) -> String {
    let mut s = String::from(HEADER);
    for row in rows {
        s.push('\n');
        s.push_str(row);
    }
    s.push('\n');
    s
}

#[sqlx::test(migrations = "./migrations")]
async fn dry_run_reports_without_inserting(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "dry@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let body = json!({ "csv_content": csv(&["特定,7203,buy,100,2500,0,2024-01-15,,ext-001"]) });
    let (status, json) = request(
        &app,
        Method::POST,
        "/import/transactions/dry-run",
        Some(&user.token),
        Some(body),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_rows"], 1);
    assert_eq!(json["to_insert"], 1);
    assert_eq!(json["errors"].as_array().unwrap().len(), 0);

    // dry-run はDBに書き込まない
    let (status, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn imports_valid_rows(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "ok@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let body = json!({
        "csv_content": csv(&[
            "特定,7203,buy,100,2500,0,2024-01-15,,ext-001",
            "特定,7203,buy,50,2600,10,2024-02-01,積立,ext-002",
        ])
    });
    let (status, json) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(body),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["inserted"], 2);
    assert_eq!(json["skipped_duplicate"], 0);

    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn skips_duplicate_by_external_id(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "extdup@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let content = csv(&["特定,7203,buy,100,2500,0,2024-01-15,,ext-001"]);

    let (_, first) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": content })),
    )
    .await;
    assert_eq!(first["inserted"], 1);

    // 同じCSVを再投入しても資産は増えない
    let (status, second) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": content })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["inserted"], 0);
    assert_eq!(second["skipped_duplicate"], 1);

    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn skips_duplicate_by_content_without_external_id(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "natdup@example.com").await;
    setup_fixtures(&app, &user.token).await;

    // external_id 空欄 → 内容の複合一致で重複判定される
    let content = csv(&["特定,7203,buy,50,2600,0,2024-02-01,,"]);

    let (_, first) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": content })),
    )
    .await;
    assert_eq!(first["inserted"], 1);

    let (status, second) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": content })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["skipped_duplicate"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_oversell(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "oversell@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let (_, _) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": csv(&["特定,7203,buy,100,2500,0,2024-01-15,,ext-001"]) })),
    )
    .await;

    // 保有100株に対して200株の売り
    let (status, _) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": csv(&["特定,7203,sell,200,2700,0,2024-03-01,,ext-002"]) })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_unknown_account_and_asset(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "unknown@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let (status, json) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": csv(&["未登録口座,7203,buy,10,100,0,2024-04-01,,ext-003"]) })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["errors"].as_array().unwrap().len(), 1);
    assert_eq!(json["errors"][0]["row"], 1);

    let (status, json) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({ "csv_content": csv(&["特定,9999,buy,10,100,0,2024-04-01,,ext-004"]) })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["errors"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn rolls_back_all_rows_when_one_is_invalid(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "rollback@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let (status, json) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({
            "csv_content": csv(&[
                "特定,7203,buy,10,2500,0,2024-05-01,,ext-004",
                "未登録口座,7203,buy,10,2500,0,2024-05-02,,ext-005",
            ])
        })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["errors"][0]["row"], 2);

    // 有効だった1行目も登録されていない
    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_duplicate_external_id_within_csv(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "csvdup@example.com").await;
    setup_fixtures(&app, &user.token).await;

    let (status, json) = request(
        &app,
        Method::POST,
        "/import/transactions",
        Some(&user.token),
        Some(json!({
            "csv_content": csv(&[
                "特定,7203,buy,10,2500,0,2024-06-01,,ext-100",
                "特定,7203,buy,20,2500,0,2024-06-02,,ext-100",
            ])
        })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["errors"][0]["row"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn requires_auth(db: PgPool) {
    let app = test_app(db);

    let (status, _) = request(
        &app,
        Method::POST,
        "/import/transactions",
        None,
        Some(json!({ "csv_content": csv(&["特定,7203,buy,10,2500,0,2024-01-15,,ext-001"]) })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
