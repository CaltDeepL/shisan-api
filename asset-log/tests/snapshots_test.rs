mod common;

use axum::http::{Method, StatusCode};
use common::{JOB_TOKEN, register_user, request, test_app_with_job_token};
use serde_json::{Value, json};
use sqlx::PgPool;

const RANGE: &str = "/analytics/asset-history?from=2024-01-01&to=2024-01-31";

/// 特定口座 + トヨタ株。1/10に100株、1/25に50株。価格は1/15,1/20,1/31。
/// 1/10〜1/14 は価格未登録期間になる。
async fn setup_portfolio(app: &axum::Router, token: &str) {
    let (status, account) = request(
        app,
        Method::POST,
        "/accounts",
        Some(token),
        Some(json!({
            "name": "特定", "account_type": "tokutei",
            "withholding": true, "currency": "JPY"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{account}");

    let (status, asset) = request(
        app,
        Method::POST,
        "/assets",
        Some(token),
        Some(json!({
            "symbol": "7203", "name": "トヨタ",
            "asset_class": "equity", "currency": "JPY"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{asset}");
    let asset_id = asset["id"].as_str().expect("asset id");

    let (status, res) = request(
        app,
        Method::POST,
        "/prices",
        Some(token),
        Some(json!({
            "asset_id": asset_id,
            "prices": [
                { "priced_on": "2024-01-15", "price": "2500" },
                { "priced_on": "2024-01-20", "price": "2600" },
                { "priced_on": "2024-01-31", "price": "2700" }
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    import(app, token, "2024-01-10", "100", "2400", "snap-001").await;
    import(app, token, "2024-01-25", "50", "2550", "snap-002").await;
}

async fn import(app: &axum::Router, token: &str, on: &str, qty: &str, price: &str, ext: &str) {
    let csv = format!(
        "account,symbol,kind,quantity,price,fee,traded_at,note,external_id\n\
         特定,7203,buy,{qty},{price},0,{on},,{ext}\n"
    );
    let (status, res) = request(
        app,
        Method::POST,
        "/import/transactions",
        Some(token),
        Some(json!({ "csv_content": csv })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

async fn run_batch(app: &axum::Router, body: Option<Value>) -> (StatusCode, Value) {
    request(app, Method::POST, "/snapshots/run", Some(JOB_TOKEN), body).await
}

fn full_range() -> Value {
    json!({ "from": "2024-01-01", "to": "2024-01-31" })
}

#[sqlx::test(migrations = "./migrations")]
async fn run_creates_snapshots(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-create@example.com").await;
    setup_portfolio(&app, &user.token).await;

    let (status, report) = run_batch(&app, Some(full_range())).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["users"], 1);
    assert_eq!(report["days"], 31);
    // 1/10〜1/31 の22日分。1/1〜1/9 は保有ゼロなので行は作られない
    assert_eq!(report["rows_upserted"], 22);
    // 1/10〜1/14 は価格未登録
    assert_eq!(report["unpriced_rows"], 5);
    assert_eq!(report["skipped_users"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn run_is_idempotent(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-idem@example.com").await;
    setup_portfolio(&app, &user.token).await;

    let (_, first) = run_batch(&app, Some(full_range())).await;
    let (_, second) = run_batch(&app, Some(full_range())).await;

    assert_eq!(first, second, "2回目の実行で結果が変わりました");
}

#[sqlx::test(migrations = "./migrations")]
async fn snapshot_matches_computed(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-match@example.com").await;
    setup_portfolio(&app, &user.token).await;

    let (status, computed) = request(&app, Method::GET, RANGE, Some(&user.token), None).await;
    assert_eq!(status, StatusCode::OK, "{computed}");
    assert_eq!(computed["source"], "computed");

    let (status, report) = run_batch(&app, Some(full_range())).await;
    assert_eq!(status, StatusCode::OK, "{report}");

    let (_, cached) = request(&app, Method::GET, RANGE, Some(&user.token), None).await;
    assert_eq!(cached["source"], "snapshot");

    // source 以外は完全一致すること。これが「正本とキャッシュの一致」の担保
    let mut a = computed;
    let mut b = cached;
    a["source"] = Value::Null;
    b["source"] = Value::Null;
    assert_eq!(a, b, "再計算とキャッシュで結果が一致しません");
}

#[sqlx::test(migrations = "./migrations")]
async fn backdated_trade_invalidates_cache(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-inval@example.com").await;
    setup_portfolio(&app, &user.token).await;

    run_batch(&app, Some(full_range())).await;
    let (_, cached) = request(&app, Method::GET, RANGE, Some(&user.token), None).await;
    assert_eq!(cached["source"], "snapshot");

    // 期間内の過去日に取引を追加 → その日以降が失効する
    import(&app, &user.token, "2024-01-22", "10", "2600", "snap-003").await;

    let (_, after) = request(&app, Method::GET, RANGE, Some(&user.token), None).await;
    assert_eq!(
        after["source"], "computed",
        "失効せずキャッシュが使われました"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn zero_holding_days_are_recorded(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-zero@example.com").await;
    setup_portfolio(&app, &user.token).await;

    run_batch(&app, Some(full_range())).await;

    // 1/1〜1/9 は保有ゼロだが「計算済み」として記録されるため、
    // キャッシュ経路が使われる（未計算と混同されない）
    let (_, res) = request(
        &app,
        Method::GET,
        "/analytics/asset-history?from=2024-01-01&to=2024-01-09",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(res["source"], "snapshot");
}

#[sqlx::test(migrations = "./migrations")]
async fn backfill_with_explicit_range(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-backfill@example.com").await;
    setup_portfolio(&app, &user.token).await;

    // まず後半だけ
    let (_, _) = run_batch(
        &app,
        Some(json!({ "from": "2024-01-20", "to": "2024-01-31" })),
    )
    .await;

    let (_, partial) = request(&app, Method::GET, RANGE, Some(&user.token), None).await;
    assert_eq!(
        partial["source"], "computed",
        "部分被覆でキャッシュが使われました"
    );

    // 前半を埋める
    let (_, _) = run_batch(
        &app,
        Some(json!({ "from": "2024-01-01", "to": "2024-01-19" })),
    )
    .await;

    let (_, full) = request(&app, Method::GET, RANGE, Some(&user.token), None).await;
    assert_eq!(full["source"], "snapshot");
}

#[sqlx::test(migrations = "./migrations")]
async fn unpriced_positions_are_kept(db: PgPool) {
    let app = test_app_with_job_token(db);
    let user = register_user(&app, "snap-unpriced@example.com").await;
    setup_portfolio(&app, &user.token).await;

    run_batch(&app, Some(full_range())).await;

    // 1/10〜1/14 は価格未登録。行は残り、評価額には計上されない
    let (_, res) = request(
        &app,
        Method::GET,
        "/analytics/asset-history?from=2024-01-12&to=2024-01-12",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(res["source"], "snapshot");
    let point = &res["series"][0]["points"][0];
    assert_eq!(point["market_value_jpy"], "0");
    assert_eq!(point["unpriced_asset_count"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn requires_job_token(db: PgPool) {
    let app = test_app_with_job_token(db);

    let (status, _) = request(&app, Method::POST, "/snapshots/run", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = request(
        &app,
        Method::POST,
        "/snapshots/run",
        Some("wrong-token"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // ユーザーのJWTでは通らない（バッチ専用トークンと分離されている）
    let user = register_user(&app, "snap-auth@example.com").await;
    let (status, _) = request(
        &app,
        Method::POST,
        "/snapshots/run",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
