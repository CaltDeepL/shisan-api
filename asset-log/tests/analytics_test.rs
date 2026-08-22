mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{register_user, request, test_app};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// 金額・数量は文字列で返るため、桁揃えの差を吸収して比較する
fn dec(v: &Value) -> Decimal {
    Decimal::from_str(v.as_str().expect("decimal must be a string")).expect("invalid decimal")
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).expect("invalid decimal literal")
}

async fn create_account(
    app: &Router,
    token: &str,
    name: &str,
    account_type: &str,
    withholding: Option<bool>,
) -> Uuid {
    let mut body = json!({
        "name": name,
        "account_type": account_type,
        "institution": "テスト証券",
    });
    if let Some(w) = withholding {
        body["withholding"] = json!(w);
    }

    let (status, json) = request(app, Method::POST, "/accounts", Some(token), Some(body)).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "account creation failed: {json}"
    );
    json["id"].as_str().expect("id").parse().expect("uuid")
}

/// `price_unit` は明示せず、`asset_class` による既定値（株式・ETFは1、投資信託は10000）に任せる
async fn create_asset(
    app: &Router,
    token: &str,
    symbol: &str,
    asset_class: &str,
    currency: &str,
) -> Uuid {
    let (status, json) = request(
        app,
        Method::POST,
        "/assets",
        Some(token),
        Some(json!({
            "symbol": symbol,
            "name": format!("{symbol} テスト銘柄"),
            "asset_class": asset_class,
            "currency": currency,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "asset creation failed: {json}");
    json["id"].as_str().expect("id").parse().expect("uuid")
}

async fn post_price(app: &Router, token: &str, asset_id: Uuid, price: &str, priced_on: &str) {
    let (status, json) = request(
        app,
        Method::POST,
        "/prices",
        Some(token),
        Some(json!({
            "asset_id": asset_id,
            "prices": [{ "priced_on": priced_on, "price": price }],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "price registration failed: {json}");
}

#[allow(clippy::too_many_arguments)]
async fn post_trade(
    app: &Router,
    token: &str,
    account_id: Uuid,
    asset_id: Uuid,
    kind: &str,
    quantity: &str,
    price: &str,
    traded_at: &str,
) {
    let (status, json) = request(
        app,
        Method::POST,
        "/transactions",
        Some(token),
        Some(json!({
            "account_id": account_id,
            "asset_id": asset_id,
            "kind": kind,
            "quantity": quantity,
            "price": price,
            "traded_at": traded_at,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "trade failed: {json}");
}

async fn get_history(app: &Router, token: &str, query: &str) -> (StatusCode, Value) {
    request(
        app,
        Method::GET,
        &format!("/analytics/asset-history{query}"),
        Some(token),
        None,
    )
    .await
}

fn points(body: &Value, key: &str) -> Vec<Value> {
    body["series"]
        .as_array()
        .expect("series")
        .iter()
        .find(|s| s["key"] == key)
        .unwrap_or_else(|| panic!("series {key} not found in {body}"))["points"]
        .as_array()
        .expect("points")
        .clone()
}

/// 完了条件そのもの。価格が1日分しか無くても、要求した全日に点が立つ。
#[sqlx::test]
async fn fills_missing_dates(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;

    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "10",
        "500",
        "2026-08-03",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-08-05").await;

    let (status, body) = get_history(&app, &user.token, "?from=2026-08-10&to=2026-08-16").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["granularity"], "day");

    let p = points(&body, "total");
    assert_eq!(p.len(), 7, "欠損日が generate_series で補完される");
    assert_eq!(p[0]["date"], "2026-08-10");
    assert_eq!(p[6]["date"], "2026-08-16");

    for pt in &p {
        assert_eq!(
            dec(&pt["market_value_jpy"]),
            d("5500"),
            "直近価格が横に伸びる"
        );
        assert_eq!(dec(&pt["cost_jpy"]), d("5000"));
        assert_eq!(pt["unpriced_asset_count"], 0);
    }
}

/// 取引日より前の日は保有ゼロ。点は立つが値は0。
#[sqlx::test]
async fn zero_before_first_trade_and_after_full_sell(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;

    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "10",
        "500",
        "2026-08-12",
    )
    .await;
    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "10",
        "600",
        "2026-08-14",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-08-01").await;

    let (status, body) = get_history(&app, &user.token, "?from=2026-08-11&to=2026-08-15").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let p = points(&body, "total");
    assert_eq!(p.len(), 5);
    assert_eq!(dec(&p[0]["market_value_jpy"]), d("0"), "8/11 は取引前");
    assert_eq!(dec(&p[1]["market_value_jpy"]), d("5500"), "8/12 買い");
    assert_eq!(dec(&p[3]["market_value_jpy"]), d("0"), "8/14 全売却");
    assert_eq!(dec(&p[4]["market_value_jpy"]), d("0"), "8/15 も0のまま");
    assert_eq!(dec(&p[4]["cost_jpy"]), d("0"));
}

/// 月次は月末＋期間の両端が返る。1/31 起点でも 2/28, 3/31 とずれない。
#[sqlx::test]
async fn monthly_returns_month_ends(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;

    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "10",
        "500",
        "2026-01-05",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-01-06").await;

    let (status, body) = get_history(
        &app,
        &user.token,
        "?from=2026-01-15&to=2026-04-10&granularity=month",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["granularity"], "month");

    let p = points(&body, "total");
    let dates: Vec<String> = p
        .iter()
        .map(|pt| pt["date"].as_str().expect("date").to_owned())
        .collect();
    assert_eq!(
        dates,
        vec![
            "2026-01-15",
            "2026-01-31",
            "2026-02-28",
            "2026-03-31",
            "2026-04-10"
        ],
    );
}

/// 価格が1件も無い銘柄は評価額に入らず、銘柄数だけ数えられる。
#[sqlx::test]
async fn unpriced_asset_is_counted_not_valued(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;
    let vti = create_asset(&app, &user.token, "VTI", "etf", "JPY").await;

    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "10",
        "500",
        "2026-08-01",
    )
    .await;
    post_trade(
        &app,
        &user.token,
        account,
        vti,
        "buy",
        "4",
        "250",
        "2026-08-01",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-08-01").await;

    let (status, body) = get_history(&app, &user.token, "?from=2026-08-10&to=2026-08-11").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let p = points(&body, "total");
    assert_eq!(
        dec(&p[0]["market_value_jpy"]),
        d("5500"),
        "VTIは評価に含めない"
    );
    assert_eq!(dec(&p[0]["cost_jpy"]), d("5000"), "簿価も評価できた分のみ");
    assert_eq!(p[0]["unpriced_asset_count"], 1);
}

/// group_by=account_type で系列が分かれる。
#[sqlx::test]
async fn group_by_account_type_splits_series(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let tokutei = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let nisa = create_account(&app, &user.token, "成長投資枠", "nisa_growth", None).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;

    post_trade(
        &app,
        &user.token,
        tokutei,
        voo,
        "buy",
        "10",
        "500",
        "2026-08-01",
    )
    .await;
    post_trade(
        &app,
        &user.token,
        nisa,
        voo,
        "buy",
        "4",
        "500",
        "2026-08-01",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-08-01").await;

    let (status, body) = get_history(
        &app,
        &user.token,
        "?from=2026-08-10&to=2026-08-10&group_by=account_type",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["series"].as_array().expect("series").len(), 2);

    assert_eq!(
        dec(&points(&body, "tokutei")[0]["market_value_jpy"]),
        d("5500")
    );
    assert_eq!(
        dec(&points(&body, "nisa_growth")[0]["market_value_jpy"]),
        d("2200")
    );
}

/// 未来日・期間逆転は422、未認証は401。
#[sqlx::test]
async fn invalid_range_and_auth(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let (status, body) = get_history(&app, &user.token, "?from=2026-08-01&to=2099-01-01").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["errors"][0]["field"], "to");

    let (status, body) = get_history(&app, &user.token, "?from=2026-08-10&to=2026-08-01").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["errors"][0]["field"], "from");

    let (status, _) = request(&app, Method::GET, "/analytics/asset-history", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// 簿価は約定日のレート、時価は当日のレートで換算される（判断b）。
#[sqlx::test]
async fn foreign_asset_uses_trade_date_rate(db: PgPool) {
    // 8/1〜8/10 を日次で埋める。隙間があると中抜け検出が発動して補充が走る
    for day in 1..=10 {
        let rate = if day == 10 { "160" } else { "150" };
        sqlx::query!(
            "INSERT INTO fx_rates (base, quote, rated_on, rate) VALUES ($1, $2, $3, $4)",
            "USD",
            "JPY",
            chrono::NaiveDate::from_ymd_opt(2026, 8, day).unwrap(),
            rust_decimal::Decimal::from_str(rate).unwrap(),
        )
        .execute(&db)
        .await
        .expect("seed fx");
    }

    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "USD").await;

    // 8/1 に 1株100USD で購入 → 簿価 100 × 150 = 15,000円
    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "1",
        "100",
        "2026-08-01",
    )
    .await;
    post_price(&app, &user.token, voo, "110", "2026-08-10").await;

    let (status, body) = get_history(&app, &user.token, "?from=2026-08-10&to=2026-08-10").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["fx_stale"], false,
        "キャッシュが足りているので補充は走らない"
    );

    let p = points(&body, "total");
    assert_eq!(dec(&p[0]["cost_jpy"]), d("15000"), "簿価は約定日の150円");
    assert_eq!(
        dec(&p[0]["market_value_jpy"]),
        d("17600"),
        "時価は当日の160円"
    );
    assert_eq!(p[0]["unpriced_asset_count"], 0);
}
