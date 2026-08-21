mod common;

use asset_log::provider::cached_fx::StalePolicy;
use axum::http::{Method, StatusCode};
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use common::{register_user, request, test_app, test_app_with_fx};
use serde_json::json;
use sqlx::PgPool;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

/// Frankfurter の成功レスポンスを組み立てる。
/// `date` は「ECB が公表した日」で、要求日と一致するとは限らない。
fn ok_body(date: NaiveDate, quote: &str, rate: &str) -> serde_json::Value {
    json!({
        "amount": 1.0,
        "base": "USD",
        "date": date.format("%Y-%m-%d").to_string(),
        "rates": { quote: rate.parse::<f64>().expect("rate") }
    })
}

async fn count_cached(db: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT count(*) as "c!" FROM fx_rates"#)
        .fetch_one(db)
        .await
        .expect("count")
}

#[sqlx::test]
async fn fetches_and_caches_rate(db: PgPool) {
    let server = MockServer::start().await;
    let on = NaiveDate::from_ymd_opt(2026, 8, 14).expect("date"); // 金曜

    // expect(1) により、2回目にヒットしたらテスト終了時に検証で落ちる
    Mock::given(method("GET"))
        .and(path("/2026-08-14"))
        .and(query_param("base", "USD"))
        .and(query_param("symbols", "JPY"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(on, "JPY", "147.25")))
        .expect(1)
        .mount(&server)
        .await;

    let app = test_app_with_fx(db.clone(), &server.uri(), StalePolicy::default());
    let user = register_user(&app, "fx@example.com").await;

    let uri = "/fx/rates?base=USD&quote=JPY&on=2026-08-14";
    let (status, body) = request(&app, Method::GET, uri, Some(&user.token), None).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rate"], "147.25", "金額は文字列で返す");
    assert_eq!(body["rated_on"], "2026-08-14");
    assert_eq!(body["is_stale"], false);
    assert_eq!(count_cached(&db).await, 1, "取得したレートが永続化される");

    // 2回目はキャッシュから返るので外部を叩かない
    let (status, body) = request(&app, Method::GET, uri, Some(&user.token), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rate"], "147.25");
    assert_eq!(count_cached(&db).await, 1);
}

#[sqlx::test]
async fn stores_response_date_not_requested_date(db: PgPool) {
    let server = MockServer::start().await;
    let friday = NaiveDate::from_ymd_opt(2026, 8, 14).expect("date");

    // 土曜を要求しても、ECB は直前営業日（金）のレートを date に入れて返す
    Mock::given(method("GET"))
        .and(path("/2026-08-15"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(friday, "JPY", "147.25")))
        .mount(&server)
        .await;

    let app = test_app_with_fx(db.clone(), &server.uri(), StalePolicy::default());
    let user = register_user(&app, "fx@example.com").await;

    let (status, body) = request(
        &app,
        Method::GET,
        "/fx/rates?base=USD&quote=JPY&on=2026-08-15",
        Some(&user.token),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    // 要求日(8/15)ではなく応答の date(8/14) が採用される
    assert_eq!(body["rated_on"], "2026-08-14");
    // 直近営業日の値を返すのは ECB の正常動作であって、フォールバックではない
    assert_eq!(body["is_stale"], false);

    let stored = sqlx::query_scalar!(r#"SELECT rated_on FROM fx_rates"#)
        .fetch_one(&db)
        .await
        .expect("stored");
    assert_eq!(stored, friday, "8/15 のレートとして保存してはいけない");
}

#[sqlx::test]
async fn falls_back_to_cache_on_5xx(db: PgPool) {
    let server = MockServer::start().await;

    // 昨日の EUR/JPY を取得してキャッシュを作る。
    // 固定日付にすると今日との距離が日々変わるので相対日付にする
    let cached_on = Utc::now().date_naive() - ChronoDuration::days(1);
    let cached_str = cached_on.format("%Y-%m-%d").to_string();

    let ok = Mock::given(method("GET"))
        .and(path(format!("/{cached_str}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "amount": 1.0, "base": "EUR", "date": cached_str,
            "rates": { "JPY": 172.5 }
        })))
        .mount_as_scoped(&server)
        .await;

    let app = test_app_with_fx(db.clone(), &server.uri(), StalePolicy::default());
    let user = register_user(&app, "fx@example.com").await;

    let (status, body) = request(
        &app,
        Method::GET,
        &format!("/fx/rates?base=EUR&quote=JPY&on={cached_str}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    // 当日分は未取得なので必ず外部に問い合わせ、そこで障害に当たる
    let (status, _body) = request(
        &app,
        Method::GET,
        "/fx/rates?base=EUR&quote=JPY",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    drop(ok); // ここから外部APIは 5xx を返す

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    // 当日分は未取得なので必ず外部に問い合わせ、そこで障害に当たる
    let (status, body) = request(
        &app,
        Method::GET,
        "/fx/rates?base=EUR&quote=JPY",
        Some(&user.token),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rate"], "172.5", "キャッシュ済みの昨日の値でしのぐ");
    assert_eq!(body["rated_on"], cached_str);
    assert_eq!(body["is_stale"], true, "古い値を返したことを明示する");
}

#[sqlx::test]
async fn falls_back_on_timeout(db: PgPool) {
    let server = MockServer::start().await;

    let cached_on = Utc::now().date_naive() - ChronoDuration::days(1);
    let cached_str = cached_on.format("%Y-%m-%d").to_string();

    let ok = Mock::given(method("GET"))
        .and(path(format!("/{cached_str}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "amount": 1.0, "base": "EUR", "date": cached_str,
            "rates": { "JPY": 172.5 }
        })))
        .mount_as_scoped(&server)
        .await;

    let app = test_app_with_fx(db.clone(), &server.uri(), StalePolicy::default());
    let user = register_user(&app, "fx@example.com").await;

    let (status, _) = request(
        &app,
        Method::GET,
        &format!("/fx/rates?base=EUR&quote=JPY&on={cached_str}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    drop(ok);

    // クライアントのタイムアウト(500ms)を超える遅延
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(2)))
        .mount(&server)
        .await;

    let (status, body) = request(
        &app,
        Method::GET,
        "/fx/rates?base=EUR&quote=JPY",
        Some(&user.token),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rated_on"], cached_str);
    assert_eq!(body["is_stale"], true);
}

#[sqlx::test]
async fn rejects_when_cache_too_old(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    // 方針の範囲外になる古いレートだけを仕込む
    let today = Utc::now().date_naive();
    let stale_on = today - ChronoDuration::days(30);
    sqlx::query!(
        "INSERT INTO fx_rates (base, quote, rated_on, rate) VALUES ('USD', 'JPY', $1, 150)",
        stale_on
    )
    .execute(&db)
    .await
    .expect("seed");

    let app = test_app_with_fx(db.clone(), &server.uri(), StalePolicy::default());
    let user = register_user(&app, "fx@example.com").await;

    let (status, body) = request(
        &app,
        Method::GET,
        "/fx/rates?base=USD&quote=JPY",
        Some(&user.token),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["type"], "/errors/service-unavailable");
    // 5xx でも利用者が取れる行動は伝える
    assert!(
        body["detail"]
            .as_str()
            .expect("detail")
            .contains("時間をおいて")
    );
}

#[sqlx::test]
async fn invalid_input_and_auth(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "fx@example.com").await;

    let (status, _) = request(
        &app,
        Method::GET,
        "/fx/rates?base=USD&quote=JPY",
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "未認証で外部APIを叩かせない"
    );

    let (status, body) = request(
        &app,
        Method::GET,
        "/fx/rates?base=US&quote=JPY",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["errors"][0]["field"], "base");

    let future = (Utc::now().date_naive() + ChronoDuration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let (status, body) = request(
        &app,
        Method::GET,
        &format!("/fx/rates?base=USD&quote=JPY&on={future}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["errors"][0]["field"], "on");

    // base のみ指定 → Query の必須フィールド不足で 400
    let (status, _) = request(
        &app,
        Method::GET,
        "/fx/rates?base=USD",
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
