mod common;
use axum::{
    Router,
    http::{Method, StatusCode},
};
use chrono::{Duration, Utc};
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

async fn create_asset(app: &Router, token: &str, symbol: &str) -> Uuid {
    let (status, json) = request(
        app,
        Method::POST,
        "/assets",
        Some(token),
        Some(json!({
            "symbol": symbol,
            "name": format!("{symbol} テスト銘柄"),
            "asset_class": "etf",
            "currency": "JPY",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "asset creation failed: {json}");
    json["id"].as_str().expect("id").parse().expect("uuid")
}

/// 取引を1件投げる。fee は 0 固定（手数料の計算自体は domain のユニットテストで検証済み）
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
) -> (StatusCode, Value) {
    request(
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
    .await
}

#[sqlx::test]
async fn create_and_list_transaction(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let other_account =
        create_account(&app, &user.token, "つみたてNISA", "nisa_tsumitate", None).await;
    let voo = create_asset(&app, &user.token, "VOO").await;
    let vti = create_asset(&app, &user.token, "VTI").await;

    let (status, created) = post_trade(
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
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(dec(&created["quantity"]), Decimal::from(10));
    assert_eq!(created["kind"], "buy");
    assert!(
        created.get("user_id").is_none(),
        "user_id を返してはいけない"
    );

    let (status, _) = post_trade(
        &app,
        &user.token,
        other_account,
        vti,
        "buy",
        "3",
        "200",
        "2026-02-10",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 一覧は約定日の降順
    let (status, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().expect("array").len(), 2);
    assert_eq!(list[0]["traded_at"], "2026-02-10", "新しい取引が先頭に来る");

    // 口座・銘柄・期間での絞り込み
    for (uri, expected) in [
        (format!("/transactions?account_id={account}"), 1),
        (format!("/transactions?asset_id={vti}"), 1),
        ("/transactions?from=2026-02-01".to_owned(), 1),
        ("/transactions?to=2026-01-31".to_owned(), 1),
        ("/transactions?from=2026-01-01&to=2026-12-31".to_owned(), 2),
    ] {
        let (status, filtered) = request(&app, Method::GET, &uri, Some(&user.token), None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(
            filtered.as_array().expect("array").len(),
            expected,
            "フィルタ {uri} の件数が想定と違う: {filtered}"
        );
    }

    // 単体取得
    let id = created["id"].as_str().expect("id");
    let (status, one) = request(
        &app,
        Method::GET,
        &format!("/transactions/{id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["id"], created["id"]);

    // from > to
    let (status, _) = request(
        &app,
        Method::GET,
        "/transactions?from=2026-12-31&to=2026-01-01",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// 完了条件そのもの
#[sqlx::test]
async fn oversell_is_rejected(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(false)).await;
    let voo = create_asset(&app, &user.token, "VOO").await;

    let (status, _) = post_trade(
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
    assert_eq!(status, StatusCode::CREATED);

    let (status, err) = post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "15",
        "600",
        "2026-02-05",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");

    // ロールバックされ、超過した売却は残っていない
    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(list.as_array().expect("array").len(), 1);

    // 保有分ちょうどの売却は通る
    let (status, _) = post_trade(
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
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn positions_are_isolated_per_account(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let nisa = create_account(&app, &user.token, "成長投資枠", "nisa_growth", None).await;
    let tokutei = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO").await;

    let (status, _) = post_trade(
        &app,
        &user.token,
        nisa,
        voo,
        "buy",
        "10",
        "500",
        "2026-01-05",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 同じ銘柄でも口座が違えば保有数量は合算されない
    let (status, err) = post_trade(
        &app,
        &user.token,
        tokutei,
        voo,
        "sell",
        "5",
        "600",
        "2026-02-05",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
}

#[sqlx::test]
async fn backdated_sell_is_validated(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO").await;

    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "10",
        "500",
        "2026-03-01",
    )
    .await;
    let (status, _) = post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "10",
        "600",
        "2026-03-05",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 途中の日付に売却を差し込むと、後続の全売却が成立しなくなる
    let (status, err) = post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "5",
        "550",
        "2026-03-03",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");

    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(
        list.as_array().expect("array").len(),
        2,
        "差し込みは残らない"
    );
}

#[sqlx::test]
async fn rebuy_after_full_sell_is_allowed(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO").await;

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

    let (status, _) = post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "3",
        "700",
        "2026-03-05",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "全売却後の再購入は通る");

    // 再購入分を超える売却は弾かれる（過去の保有数量が復活していないこと）
    let (status, _) = post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "4",
        "700",
        "2026-04-05",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn delete_recalculates_position(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO").await;

    let (_, first_buy) = post_trade(
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
    let (_, second_buy) = post_trade(
        &app,
        &user.token,
        account,
        voo,
        "buy",
        "5",
        "520",
        "2026-02-05",
    )
    .await;
    post_trade(
        &app,
        &user.token,
        account,
        voo,
        "sell",
        "6",
        "600",
        "2026-03-05",
    )
    .await;

    // 10株の買いを消すと、残り5株では6株の売却が成立しない
    let first_id = first_buy["id"].as_str().expect("id");
    let (status, err) = request(
        &app,
        Method::DELETE,
        &format!("/transactions/{first_id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");

    // 5株の買いなら消せる
    let second_id = second_buy["id"].as_str().expect("id");
    let (status, _) = request(
        &app,
        Method::DELETE,
        &format!("/transactions/{second_id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = request(&app, Method::GET, "/transactions", Some(&user.token), None).await;
    assert_eq!(list.as_array().expect("array").len(), 2);

    // 消した取引の再取得は404
    let (status, _) = request(
        &app,
        Method::GET,
        &format!("/transactions/{second_id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn other_users_account_or_asset_is_not_found(db: PgPool) {
    let app = test_app(db);
    let owner = register_user(&app, "owner@example.com").await;
    let intruder = register_user(&app, "intruder@example.com").await;

    let owner_account = create_account(&app, &owner.token, "特定口座", "tokutei", Some(true)).await;
    let owner_asset = create_asset(&app, &owner.token, "VOO").await;
    let (_, owned) = post_trade(
        &app,
        &owner.token,
        owner_account,
        owner_asset,
        "buy",
        "10",
        "500",
        "2026-01-05",
    )
    .await;

    let mine_account =
        create_account(&app, &intruder.token, "特定口座", "tokutei", Some(true)).await;
    let mine_asset = create_asset(&app, &intruder.token, "VTI").await;

    // 他人の口座を指定
    let (status, _) = post_trade(
        &app,
        &intruder.token,
        owner_account,
        mine_asset,
        "buy",
        "1",
        "100",
        "2026-01-05",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 他人の銘柄を指定
    let (status, _) = post_trade(
        &app,
        &intruder.token,
        mine_account,
        owner_asset,
        "buy",
        "1",
        "100",
        "2026-01-05",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 他人の取引は取得も削除もできず、一覧にも出ない
    let owned_id = owned["id"].as_str().expect("id");
    for method in [Method::GET, Method::DELETE] {
        let (status, _) = request(
            &app,
            method,
            &format!("/transactions/{owned_id}"),
            Some(&intruder.token),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    let (_, list) = request(
        &app,
        Method::GET,
        "/transactions",
        Some(&intruder.token),
        None,
    )
    .await;
    assert!(list.as_array().expect("array").is_empty());
}

#[sqlx::test]
async fn invalid_input_is_rejected(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;
    let account = create_account(&app, &user.token, "特定口座", "tokutei", Some(true)).await;
    let voo = create_asset(&app, &user.token, "VOO").await;

    let future = (Utc::now().date_naive() + Duration::days(2)).to_string();
    let cases = [
        (
            "数量が0",
            json!({"quantity": "0", "price": "500", "traded_at": "2026-01-05"}),
        ),
        (
            "数量が負",
            json!({"quantity": "-1", "price": "500", "traded_at": "2026-01-05"}),
        ),
        (
            "価格が負",
            json!({"quantity": "1", "price": "-500", "traded_at": "2026-01-05"}),
        ),
        (
            "未来日",
            json!({"quantity": "1", "price": "500", "traded_at": future}),
        ),
    ];

    for (label, patch) in cases {
        let mut body = json!({
            "account_id": account,
            "asset_id": voo,
            "kind": "buy",
        });
        for (k, v) in patch.as_object().expect("object") {
            body[k] = v.clone();
        }

        let (status, err) = request(
            &app,
            Method::POST,
            "/transactions",
            Some(&user.token),
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{label}: {err}");
    }

    // 空白のみのメモ
    let (status, err) = request(
        &app,
        Method::POST,
        "/transactions",
        Some(&user.token),
        Some(json!({
            "account_id": account,
            "asset_id": voo,
            "kind": "buy",
            "quantity": "1",
            "price": "500",
            "traded_at": "2026-01-05",
            "note": "   ",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
}

#[sqlx::test]
async fn requires_authentication(db: PgPool) {
    let app = test_app(db);

    let (status, _) = request(&app, Method::GET, "/transactions", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = request(
        &app,
        Method::POST,
        "/transactions",
        None,
        Some(json!({
            "account_id": Uuid::new_v4(),
            "asset_id": Uuid::new_v4(),
            "kind": "buy",
            "quantity": "1",
            "price": "500",
            "traded_at": "2026-01-05",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = request(
        &app,
        Method::DELETE,
        &format!("/transactions/{}", Uuid::new_v4()),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
