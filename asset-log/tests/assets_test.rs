mod common;
use axum::http::{Method, StatusCode};
use common::{register_user, request, test_app};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// 金額は文字列で返るため、桁揃え（"1" と "1.00000000"）の差を吸収して比較する
fn dec(v: &Value) -> Decimal {
    Decimal::from_str(v.as_str().expect("decimal must be a string")).expect("invalid decimal")
}

#[sqlx::test]
async fn create_and_search_asset(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let (status, voo) = request(
        &app,
        Method::POST,
        "/assets",
        Some(&user.token),
        Some(json!({
            "symbol": "VOO",
            "name": "Vanguard S&P 500 ETF",
            "asset_class": "etf",
            "currency": "usd"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{voo}");
    assert_eq!(voo["currency"], "USD", "小文字の通貨コードは正規化される");
    assert!(voo.get("user_id").is_none(), "user_id を返してはいけない");

    let (status, _) = request(
        &app,
        Method::POST,
        "/assets",
        Some(&user.token),
        Some(json!({
            "symbol": "0331418A",
            "name": "eMAXIS Slim 全世界株式",
            "asset_class": "mutual_fund"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, list) = request(&app, Method::GET, "/assets", Some(&user.token), None).await;
    assert_eq!(status, StatusCode::OK);
    let list = list.as_array().expect("array");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["symbol"], "0331418A", "upper(symbol) 昇順");

    // 名前の部分一致・大文字小文字を無視
    let (_, hit) = request(
        &app,
        Method::GET,
        "/assets?q=vanguard",
        Some(&user.token),
        None,
    )
    .await;
    let hit = hit.as_array().expect("array");
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0]["symbol"], "VOO");

    let id = voo["id"].as_str().unwrap();
    let (status, one) = request(
        &app,
        Method::GET,
        &format!("/assets/{id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["name"], "Vanguard S&P 500 ETF");
}

#[sqlx::test]
async fn duplicate_symbol_conflicts(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "dup@example.com").await;

    let body = json!({ "symbol": "VOO", "name": "ETF", "asset_class": "etf" });
    let (status, _) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);

    // 大文字小文字違いでも同一銘柄とみなす（assets_user_symbol_key は upper(symbol)）
    let body = json!({ "symbol": "voo", "name": "別名", "asset_class": "etf" });
    let (status, err) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{err}");
}

#[sqlx::test]
async fn other_users_asset_is_not_found(db: PgPool) {
    let app = test_app(db);
    let owner = register_user(&app, "owner2@example.com").await;
    let stranger = register_user(&app, "stranger@example.com").await;

    let body = json!({ "symbol": "VOO", "name": "ETF", "asset_class": "etf" });
    let (_, asset) = request(
        &app,
        Method::POST,
        "/assets",
        Some(&owner.token),
        Some(body),
    )
    .await;
    let id = asset["id"].as_str().unwrap();

    let (status, _) = request(
        &app,
        Method::GET,
        &format!("/assets/{id}"),
        Some(&stranger.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = request(
        &app,
        Method::PATCH,
        &format!("/assets/{id}"),
        Some(&stranger.token),
        Some(json!({ "name": "乗っ取り" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, list) = request(&app, Method::GET, "/assets", Some(&stranger.token), None).await;
    assert!(
        list.as_array().unwrap().is_empty(),
        "一覧にも出てはいけない"
    );

    // 同じ symbol を他人が登録できること（UNIQUE はユーザー単位）
    let body = json!({ "symbol": "VOO", "name": "ETF", "asset_class": "etf" });
    let (status, _) = request(
        &app,
        Method::POST,
        "/assets",
        Some(&stranger.token),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn invalid_input_is_rejected(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "invalid@example.com").await;

    let cases = [
        json!({ "symbol": "A", "name": "x", "asset_class": "etf", "price_unit": "0" }),
        json!({ "symbol": "B", "name": "x", "asset_class": "etf", "currency": "US" }),
        json!({ "symbol": "   ", "name": "x", "asset_class": "etf" }),
        json!({ "symbol": "C", "name": "  ", "asset_class": "etf" }),
    ];

    for body in cases {
        let (status, err) = request(
            &app,
            Method::POST,
            "/assets",
            Some(&user.token),
            Some(body.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "should be 422: {body} -> {err}"
        );
    }

    // 空の PATCH は 400
    let body = json!({ "symbol": "D", "name": "x", "asset_class": "etf" });
    let (_, asset) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    let id = asset["id"].as_str().unwrap();

    let (status, _) = request(
        &app,
        Method::PATCH,
        &format!("/assets/{id}"),
        Some(&user.token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn price_unit_defaults_by_asset_class(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "unit@example.com").await;

    // 投信の基準価額は1万口あたり。既定値が1になると評価額が1万倍になる
    let body = json!({ "symbol": "0331418A", "name": "投信", "asset_class": "mutual_fund" });
    let (_, fund) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    assert_eq!(dec(&fund["price_unit"]), Decimal::from(10_000));

    let body = json!({ "symbol": "7203", "name": "トヨタ", "asset_class": "equity" });
    let (_, stock) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    assert_eq!(dec(&stock["price_unit"]), Decimal::ONE);

    // 明示指定は既定値より優先される
    let body = json!({
        "symbol": "9999", "name": "特殊", "asset_class": "mutual_fund", "price_unit": "1"
    });
    let (_, custom) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    assert_eq!(dec(&custom["price_unit"]), Decimal::ONE);
}

#[sqlx::test]
async fn price_upsert_overwrites_same_day(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "price@example.com").await;

    let body = json!({ "symbol": "VOO", "name": "ETF", "asset_class": "etf", "currency": "USD" });
    let (_, asset) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    let asset_id = asset["id"].as_str().unwrap().to_owned();

    let post = |price: &str, date: &str| {
        json!({
            "asset_id": asset_id,
            "prices": [{ "priced_on": date, "price": price }]
        })
    };

    let (status, res) = request(
        &app,
        Method::POST,
        "/prices",
        Some(&user.token),
        Some(post("500.00", "2026-08-20")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["upserted"], 1);

    // 同じ日を訂正
    let (status, _) = request(
        &app,
        Method::POST,
        "/prices",
        Some(&user.token),
        Some(post("505.50", "2026-08-20")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, history) = request(
        &app,
        Method::GET,
        &format!("/prices/{asset_id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let history = history.as_array().expect("array");
    assert_eq!(history.len(), 1, "行が増えず上書きされる");
    assert_eq!(
        dec(&history[0]["price"]),
        Decimal::from_str("505.50").unwrap()
    );

    // 同一リクエスト内に同じ日付が2件あっても 21000 で落ちず、後勝ちになる
    let body = json!({
        "asset_id": asset_id,
        "prices": [
            { "priced_on": "2026-08-19", "price": "1" },
            { "priced_on": "2026-08-19", "price": "2" }
        ]
    });
    let (status, res) = request(&app, Method::POST, "/prices", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{res}");

    let (_, history) = request(
        &app,
        Method::GET,
        &format!("/prices/{asset_id}?from=2026-08-19&to=2026-08-19"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(dec(&history[0]["price"]), Decimal::from(2));
}

#[sqlx::test]
async fn price_batch_is_all_or_nothing(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "batch@example.com").await;

    let body = json!({ "symbol": "VOO", "name": "ETF", "asset_class": "etf" });
    let (_, asset) = request(&app, Method::POST, "/assets", Some(&user.token), Some(body)).await;
    let asset_id = asset["id"].as_str().unwrap().to_owned();

    let body = json!({
        "asset_id": asset_id,
        "prices": [
            { "priced_on": "2026-08-18", "price": "100" },
            { "priced_on": "2026-08-19", "price": "-1" }
        ]
    });
    let (status, _) = request(&app, Method::POST, "/prices", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, history) = request(
        &app,
        Method::GET,
        &format!("/prices/{asset_id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert!(
        history.as_array().unwrap().is_empty(),
        "1件でも不正なら正常な行も登録されない"
    );

    // 空配列は 400
    let body = json!({ "asset_id": asset_id, "prices": [] });
    let (status, _) = request(&app, Method::POST, "/prices", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 未来日は 422（DB の CHECK では表現できないため handler が守る）
    let body = json!({
        "asset_id": asset_id,
        "prices": [{ "priced_on": "2999-01-01", "price": "100" }]
    });
    let (status, _) = request(&app, Method::POST, "/prices", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn price_requires_owned_asset(db: PgPool) {
    let app = test_app(db);
    let owner = register_user(&app, "owner3@example.com").await;
    let stranger = register_user(&app, "stranger3@example.com").await;

    let body = json!({ "symbol": "VOO", "name": "ETF", "asset_class": "etf" });
    let (_, asset) = request(
        &app,
        Method::POST,
        "/assets",
        Some(&owner.token),
        Some(body),
    )
    .await;
    let asset_id = asset["id"].as_str().unwrap().to_owned();

    let body = json!({
        "asset_id": asset_id,
        "prices": [{ "priced_on": "2026-08-20", "price": "100" }]
    });
    let (status, _) = request(
        &app,
        Method::POST,
        "/prices",
        Some(&stranger.token),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = request(
        &app,
        Method::GET,
        &format!("/prices/{asset_id}"),
        Some(&stranger.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 存在しない銘柄も 404
    let missing = Uuid::new_v4();
    let body = json!({
        "asset_id": missing,
        "prices": [{ "priced_on": "2026-08-20", "price": "100" }]
    });
    let (status, _) = request(
        &app,
        Method::POST,
        "/prices",
        Some(&owner.token),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn requires_authentication(db: PgPool) {
    let app = test_app(db);

    for (method, uri) in [
        (Method::GET, "/assets"),
        (Method::POST, "/assets"),
        (Method::POST, "/prices"),
    ] {
        let (status, _) = request(&app, method, uri, None, Some(json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}
