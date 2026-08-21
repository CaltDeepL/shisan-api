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

async fn get_holdings(app: &Router, token: &str, query: &str) -> (StatusCode, Value) {
    request(
        app,
        Method::GET,
        &format!("/holdings{query}"),
        Some(token),
        None,
    )
    .await
}

/// 完了条件そのもの。買い建てた銘柄に現在価格が当たり、評価損益が返る。
/// あわせて、価格が複数日あるとき最新日が使われることを確認する。
#[sqlx::test]
async fn holdings_returns_valuation(db: PgPool) {
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

    // 古い日 → 新しい日 の順に入れ、最新日が採用されることを見る
    post_price(&app, &user.token, voo, "480", "2026-01-31").await;
    post_price(&app, &user.token, voo, "550", "2026-08-20").await;

    let (status, body) = get_holdings(&app, &user.token, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let holdings = body["holdings"].as_array().expect("holdings");
    assert_eq!(holdings.len(), 1);

    let h = &holdings[0];
    assert_eq!(h["symbol"], "VOO");
    assert_eq!(h["account_type"], "tokutei");
    assert_eq!(dec(&h["quantity"]), d("10"));
    assert_eq!(dec(&h["avg_cost"]), d("500"));
    assert_eq!(dec(&h["book_value"]), d("5000"));
    assert_eq!(dec(&h["realized_pnl"]), d("0"));

    assert_eq!(dec(&h["price"]), d("550"), "最新日の価格が使われる");
    assert_eq!(h["priced_on"], "2026-08-20");
    assert_eq!(dec(&h["market_value"]), d("5500"));
    assert_eq!(dec(&h["unrealized_pnl"]), d("500"));
    assert_eq!(dec(&h["unrealized_pnl_rate"]), d("0.1"));

    let totals = body["summary"]["totals"].as_array().expect("totals");
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0]["currency"], "JPY");
    assert_eq!(dec(&totals[0]["book_value"]), d("5000"));
    assert_eq!(dec(&totals[0]["market_value"]), d("5500"));
    assert_eq!(dec(&totals[0]["unrealized_pnl"]), d("500"));
    assert_eq!(body["summary"]["unpriced_count"], 0);
}

/// 投資信託は10,000口あたりの基準価額で登録されるため、
/// 評価額は `数量 × 価格 ÷ price_unit` になる。
#[sqlx::test]
async fn mutual_fund_valuation_uses_price_unit(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "つみたてNISA", "nisa_tsumitate", None).await;
    let fund = create_asset(&app, &user.token, "EMAXIS", "mutual_fund", "JPY").await;

    // 10,000口を基準価額11,000円で購入 → 取得原価は 10000 × 11000 ÷ 10000 = 11,000円
    post_trade(
        &app,
        &user.token,
        account,
        fund,
        "buy",
        "10000",
        "11000",
        "2026-01-05",
    )
    .await;
    post_price(&app, &user.token, fund, "12000", "2026-08-20").await;

    let (status, body) = get_holdings(&app, &user.token, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let h = &body["holdings"][0];
    assert_eq!(dec(&h["price_unit"]), d("10000"));
    assert_eq!(dec(&h["quantity"]), d("10000"));
    assert_eq!(dec(&h["book_value"]), d("11000"));
    assert_eq!(dec(&h["market_value"]), d("12000"));
    assert_eq!(dec(&h["unrealized_pnl"]), d("1000"));
}

/// 価格が1件も無い銘柄は、保有行は返るが評価系が null になる。
/// 合計の騰落率の分母は「評価できた銘柄の簿価」だけであることも確認する。
#[sqlx::test]
async fn unpriced_asset_returns_nulls(db: PgPool) {
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
        "2026-01-05",
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
        "2026-01-05",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-08-20").await;

    let (status, body) = get_holdings(&app, &user.token, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let holdings = body["holdings"].as_array().expect("holdings");
    assert_eq!(holdings.len(), 2, "価格が無くても一覧からは消えない");

    let vti_row = holdings
        .iter()
        .find(|h| h["symbol"] == "VTI")
        .expect("VTI row");
    assert_eq!(dec(&vti_row["book_value"]), d("1000"));
    assert!(vti_row["price"].is_null());
    assert!(vti_row["priced_on"].is_null());
    assert!(vti_row["market_value"].is_null());
    assert!(vti_row["unrealized_pnl"].is_null());
    assert!(vti_row["unrealized_pnl_rate"].is_null());

    let summary = &body["summary"];
    assert_eq!(summary["unpriced_count"], 1);

    let totals = &summary["totals"][0];
    assert_eq!(
        dec(&totals["book_value"]),
        d("6000"),
        "簿価は未評価分も含む"
    );
    assert_eq!(
        dec(&totals["market_value"]),
        d("5500"),
        "評価額は価格のある銘柄のみ"
    );
    assert_eq!(totals["unpriced_count"], 1);
    assert_eq!(
        dec(&totals["unrealized_pnl_rate"]),
        d("0.1"),
        "騰落率の分母は評価できた5000のみ。6000で割ると実際より低く出る"
    );
}

/// 全売却済みは既定で非表示。`include_closed=true` で実現損益を取り出せる。
#[sqlx::test]
async fn closed_position_is_hidden_by_default(db: PgPool) {
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
    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "10",
        "600",
        "2026-02-05",
    )
    .await;
    post_price(&app, &user.token, voo, "550", "2026-08-20").await;

    let (status, body) = get_holdings(&app, &user.token, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["holdings"].as_array().expect("holdings").is_empty(),
        "既定では数量0のポジションは返らない"
    );
    assert!(
        body["summary"]["totals"]
            .as_array()
            .expect("totals")
            .is_empty()
    );

    let (status, body) = get_holdings(&app, &user.token, "?include_closed=true").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let holdings = body["holdings"].as_array().expect("holdings");
    assert_eq!(holdings.len(), 1);
    assert_eq!(dec(&holdings[0]["quantity"]), d("0"));
    assert_eq!(dec(&holdings[0]["book_value"]), d("0"));
    assert_eq!(
        dec(&holdings[0]["realized_pnl"]),
        d("1000"),
        "10 × 600 − 取得原価5000"
    );
}

/// JPY と USD は合算しない。合計は通貨ごとの配列で返る。
#[sqlx::test]
async fn totals_are_split_by_currency(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let jp = create_asset(&app, &user.token, "1306", "etf", "JPY").await;
    let us = create_asset(&app, &user.token, "VOO", "etf", "USD").await;

    post_trade(
        &app,
        &user.token,
        account,
        jp,
        "buy",
        "10",
        "500",
        "2026-01-05",
    )
    .await;
    post_trade(
        &app,
        &user.token,
        account,
        us,
        "buy",
        "2",
        "100",
        "2026-01-05",
    )
    .await;
    post_price(&app, &user.token, jp, "550", "2026-08-20").await;
    post_price(&app, &user.token, us, "120", "2026-08-20").await;

    let (status, body) = get_holdings(&app, &user.token, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let totals = body["summary"]["totals"].as_array().expect("totals");
    assert_eq!(totals.len(), 2, "通貨ごとに分かれる");

    let jpy = totals
        .iter()
        .find(|t| t["currency"] == "JPY")
        .expect("JPY totals");
    assert_eq!(dec(&jpy["book_value"]), d("5000"));
    assert_eq!(dec(&jpy["market_value"]), d("5500"));

    let usd = totals
        .iter()
        .find(|t| t["currency"] == "USD")
        .expect("USD totals");
    assert_eq!(dec(&usd["book_value"]), d("200"));
    assert_eq!(dec(&usd["market_value"]), d("240"));
}

/// 同じ銘柄を複数口座で持つと別行になり、口座ごとの内訳も返る。
#[sqlx::test]
async fn positions_are_grouped_per_account(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let a = create_account(&app, &user.token, "A証券", "tokutei", Some(true)).await;
    let b = create_account(&app, &user.token, "B証券", "nisa_growth", None).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;

    post_trade(&app, &user.token, a, voo, "buy", "10", "500", "2026-01-05").await;
    post_trade(&app, &user.token, b, voo, "buy", "5", "600", "2026-01-05").await;
    post_price(&app, &user.token, voo, "550", "2026-08-20").await;

    let (status, body) = get_holdings(&app, &user.token, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let holdings = body["holdings"].as_array().expect("holdings");
    assert_eq!(holdings.len(), 2, "口座をまたいで合算しない");

    // 口座名順に並ぶ
    assert_eq!(holdings[0]["account_name"], "A証券");
    assert_eq!(dec(&holdings[0]["avg_cost"]), d("500"));
    assert_eq!(holdings[1]["account_name"], "B証券");
    assert_eq!(dec(&holdings[1]["avg_cost"]), d("600"));

    let by_account = body["summary"]["by_account"]
        .as_array()
        .expect("by_account");
    assert_eq!(by_account.len(), 2);
    assert_eq!(by_account[0]["account_name"], "A証券");
    assert_eq!(dec(&by_account[0]["totals"][0]["market_value"]), d("5500"));
    assert_eq!(by_account[1]["account_name"], "B証券");
    assert_eq!(dec(&by_account[1]["totals"][0]["market_value"]), d("2750"));

    // 全体の合計は両方の和
    assert_eq!(
        dec(&body["summary"]["totals"][0]["market_value"]),
        d("8250")
    );
}

/// `?account_id=` で絞り込める。他人の・存在しない口座は 404。
#[sqlx::test]
async fn account_filter_and_unknown_account_is_404(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let other = register_user(&app, "other@example.com").await;

    let a = create_account(&app, &user.token, "A証券", "tokutei", Some(true)).await;
    let b = create_account(&app, &user.token, "B証券", "nisa_growth", None).await;
    let voo = create_asset(&app, &user.token, "VOO", "etf", "JPY").await;
    let vti = create_asset(&app, &user.token, "VTI", "etf", "JPY").await;

    post_trade(&app, &user.token, a, voo, "buy", "10", "500", "2026-01-05").await;
    post_trade(&app, &user.token, b, vti, "buy", "4", "250", "2026-01-05").await;

    let (status, body) = get_holdings(&app, &user.token, &format!("?account_id={a}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let holdings = body["holdings"].as_array().expect("holdings");
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings[0]["symbol"], "VOO");

    // 他人の口座
    let other_account =
        create_account(&app, &other.token, "他人の口座", "tokutei", Some(true)).await;
    let (status, _) =
        get_holdings(&app, &user.token, &format!("?account_id={other_account}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "他人の口座は403ではなく404");

    // 存在しない口座
    let (status, _) = get_holdings(
        &app,
        &user.token,
        &format!("?account_id={}", Uuid::new_v4()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn requires_authentication(db: PgPool) {
    let app = test_app(db);

    let (status, _) = request(&app, Method::GET, "/holdings", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
