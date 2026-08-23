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

// ---------------------------------------------------------------------------
// タスク#12: GET /analytics/allocation
// ---------------------------------------------------------------------------

async fn get_allocation(app: &Router, token: &str, query: &str) -> (StatusCode, Value) {
    request(
        app,
        Method::GET,
        &format!("/analytics/allocation{query}"),
        Some(token),
        None,
    )
    .await
}

/// `items` から `key` 一致の1件を取り出す
fn item<'a>(body: &'a Value, key: &str) -> &'a Value {
    body["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["key"] == key)
        .unwrap_or_else(|| panic!("item {key} not found in {body}"))
}

/// `ratio` の総和。完了条件の検証に使う
fn ratio_sum(body: &Value) -> Decimal {
    body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| dec(&i["ratio"]))
        .sum()
}

/// 完了条件そのもの。3等分で端数が出ても、比率の合計はちょうど 100.00 になる。
#[sqlx::test]
async fn allocation_ratios_sum_to_100(db: PgPool) {
    let app = test_app(db);
    let u = register_user(&app, "alloc-sum@example.com").await;

    let acc = create_account(&app, &u.token, "証券A", "tokutei", Some(true)).await;
    // 評価額が 1,000,000 ずつ揃うよう数量と単価を組む（33.33% が3つ = 端数が出る）
    let specs = [
        ("AAA", "100", "10000"),
        ("BBB", "200", "5000"),
        ("CCC", "400", "2500"),
    ];
    let mut ids = Vec::new();
    for (sym, qty, price) in specs {
        let id = create_asset(&app, &u.token, sym, "equity", "JPY").await;
        post_trade(&app, &u.token, acc, id, "buy", qty, price, "2026-01-05").await;
        post_price(&app, &u.token, id, price, "2026-01-10").await;
        ids.push(id);
    }

    let (status, body) = get_allocation(&app, &u.token, "?as_of=2026-01-10&group_by=asset").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["items"].as_array().expect("items").len(), 3);
    assert_eq!(
        ratio_sum(&body),
        d("100.00"),
        "比率の合計が100%でない: {body}"
    );
    assert_eq!(dec(&body["total_value_jpy"]), d("3000000"));
    assert_eq!(body["scope"], "securities_only");

    // 最大剰余法。先頭の1件だけが 0.01 を受け取る
    let ratios: Vec<Decimal> = body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| dec(&i["ratio"]))
        .collect();
    assert_eq!(ratios, vec![d("33.34"), d("33.33"), d("33.33")]);
}

/// 口座軸では key が UUID、label が登録した口座名になる。
#[sqlx::test]
async fn allocation_by_account_returns_names(db: PgPool) {
    let app = test_app(db);
    let u = register_user(&app, "alloc-acc@example.com").await;

    let tokutei = create_account(&app, &u.token, "特定口座A", "tokutei", Some(true)).await;
    let nisa = create_account(&app, &u.token, "NISA口座B", "nisa_growth", None).await;
    let asset = create_asset(&app, &u.token, "DDD", "equity", "JPY").await;

    post_trade(
        &app,
        &u.token,
        tokutei,
        asset,
        "buy",
        "300",
        "1000",
        "2026-01-05",
    )
    .await;
    post_trade(
        &app,
        &u.token,
        nisa,
        asset,
        "buy",
        "100",
        "1000",
        "2026-01-05",
    )
    .await;
    post_price(&app, &u.token, asset, "1000", "2026-01-10").await;

    let (status, body) = get_allocation(&app, &u.token, "?as_of=2026-01-10&group_by=account").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(item(&body, &tokutei.to_string())["label"], "特定口座A");
    assert_eq!(item(&body, &nisa.to_string())["label"], "NISA口座B");
    assert_eq!(dec(&item(&body, &tokutei.to_string())["ratio"]), d("75.00"));
    assert_eq!(dec(&item(&body, &nisa.to_string())["ratio"]), d("25.00"));
    assert_eq!(ratio_sum(&body), d("100.00"));
}

/// allocation は asset-history と同じ経路を通る。両者の合計が一致することを保証する。
/// これが崩れたら「折れ線の合計と円グラフの合計が合わない」バグが入っている。
#[sqlx::test]
async fn allocation_matches_history_total(db: PgPool) {
    let app = test_app(db);
    let u = register_user(&app, "alloc-match@example.com").await;

    let acc = create_account(&app, &u.token, "証券A", "tokutei", Some(true)).await;
    let equity = create_asset(&app, &u.token, "EEE", "equity", "JPY").await;
    let fund = create_asset(&app, &u.token, "FFF", "mutual_fund", "JPY").await;

    post_trade(
        &app,
        &u.token,
        acc,
        equity,
        "buy",
        "37",
        "1234",
        "2026-01-05",
    )
    .await;
    post_trade(
        &app,
        &u.token,
        acc,
        fund,
        "buy",
        "12345",
        "13579",
        "2026-01-05",
    )
    .await;
    post_price(&app, &u.token, equity, "1301", "2026-01-10").await;
    post_price(&app, &u.token, fund, "14022", "2026-01-10").await;

    let (_, alloc) = get_allocation(&app, &u.token, "?as_of=2026-01-10&group_by=asset_class").await;
    // allocation では none を弾いているが、時系列側で合計を見るのは正当な用途
    let (_, hist) = get_history(
        &app,
        &u.token,
        "?from=2026-01-10&to=2026-01-10&group_by=none",
    )
    .await;

    let hist_total = dec(&points(&hist, "total")[0]["market_value_jpy"]);
    assert_eq!(
        dec(&alloc["total_value_jpy"]),
        hist_total,
        "allocation と asset-history の合計が一致しない"
    );
    assert_eq!(ratio_sum(&alloc), d("100.00"));
}

/// 価格未登録の銘柄は分母から外れ、件数だけが返る。
#[sqlx::test]
async fn allocation_excludes_unpriced(db: PgPool) {
    let app = test_app(db);
    let u = register_user(&app, "alloc-unpriced@example.com").await;

    let acc = create_account(&app, &u.token, "証券A", "tokutei", Some(true)).await;
    let priced = create_asset(&app, &u.token, "GGG", "equity", "JPY").await;
    let unpriced = create_asset(&app, &u.token, "HHH", "equity", "JPY").await;

    post_trade(
        &app,
        &u.token,
        acc,
        priced,
        "buy",
        "100",
        "2000",
        "2026-01-05",
    )
    .await;
    post_trade(
        &app,
        &u.token,
        acc,
        unpriced,
        "buy",
        "50",
        "3000",
        "2026-01-05",
    )
    .await;
    post_price(&app, &u.token, priced, "2000", "2026-01-10").await;

    let (status, body) = get_allocation(&app, &u.token, "?as_of=2026-01-10&group_by=asset").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 価格のある1件だけが構成比を持ち、それが100%になる
    assert_eq!(body["items"].as_array().expect("items").len(), 1);
    assert_eq!(item(&body, &priced.to_string())["key"], priced.to_string());
    assert_eq!(dec(&body["total_value_jpy"]), d("200000"));
    assert_eq!(body["unpriced_asset_count"], 1);
    assert_eq!(ratio_sum(&body), d("100.00"));
}

/// 保有ゼロでも 500 にならず、空の構成比を返す。
#[sqlx::test]
async fn allocation_empty_portfolio(db: PgPool) {
    let app = test_app(db);
    let u = register_user(&app, "alloc-empty@example.com").await;
    create_account(&app, &u.token, "証券A", "tokutei", Some(true)).await;

    let (status, body) = get_allocation(&app, &u.token, "?as_of=2026-01-10").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["items"].as_array().expect("items").is_empty());
    assert_eq!(dec(&body["total_value_jpy"]), d("0"));
    assert_eq!(body["unpriced_asset_count"], 0);
    // 既定の分類軸
    assert_eq!(body["group_by"], "asset_class");
}

/// none 指定・未来日は 422、無認証は 401。
#[sqlx::test]
async fn allocation_rejects_none_and_future(db: PgPool) {
    let app = test_app(db);
    let u = register_user(&app, "alloc-invalid@example.com").await;

    let (status, body) = get_allocation(&app, &u.token, "?group_by=none").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = get_allocation(&app, &u.token, "?as_of=2999-12-31").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, _) = request(&app, Method::GET, "/analytics/allocation", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
