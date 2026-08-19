mod common;

use axum::http::{Method, StatusCode};
use common::{register_user, request, test_app};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// 作成 → 一覧 → 単体取得 の一連が通ること
#[sqlx::test(migrations = "./migrations")]
async fn create_and_fetch_account(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let body = json!({
        "name": "SBI証券 特定",
        "account_type": "tokutei",
        "withholding": true,
        "institution": "SBI証券"
    });
    let (status, created) =
        request(&app, Method::POST, "/accounts", Some(&user.token), Some(body)).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["name"], "SBI証券 特定");
    assert_eq!(created["withholding"], true);
    assert_eq!(created["currency"], "JPY");
    // user_id は外に出さない
    assert!(created.get("user_id").is_none());

    let id = created["id"].as_str().unwrap();

    let (status, list) = request(&app, Method::GET, "/accounts", Some(&user.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    let (status, one) = request(
        &app,
        Method::GET,
        &format!("/accounts/{id}"),
        Some(&user.token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["id"], id);
}

/// 特定口座で withholding を省略すると 422
#[sqlx::test(migrations = "./migrations")]
async fn tokutei_requires_withholding(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let body = json!({ "name": "源泉なし", "account_type": "tokutei" });
    let (status, json) =
        request(&app, Method::POST, "/accounts", Some(&user.token), Some(body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["type"], "/errors/unprocessable-entity");
}

/// 同一ユーザー内で口座名が重複すると 409
#[sqlx::test(migrations = "./migrations")]
async fn duplicate_name_conflicts(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let body = json!({ "name": "同じ名前", "account_type": "ippan" });
    let (status, _) = request(
        &app,
        Method::POST,
        "/accounts",
        Some(&user.token),
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, json) =
        request(&app, Method::POST, "/accounts", Some(&user.token), Some(body)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["type"], "/errors/conflict");
}

/// PATCH の三値: 未指定は据え置き、null は NULL 化
#[sqlx::test(migrations = "./migrations")]
async fn patch_distinguishes_null_and_absent(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let body = json!({
        "name": "元の名前",
        "account_type": "ippan",
        "institution": "元の証券会社"
    });
    let (_, created) =
        request(&app, Method::POST, "/accounts", Some(&user.token), Some(body)).await;
    let id = created["id"].as_str().unwrap().to_owned();
    let path = format!("/accounts/{id}");

    // name だけ変更 → institution は据え置き
    let (status, patched) = request(
        &app,
        Method::PATCH,
        &path,
        Some(&user.token),
        Some(json!({ "name": "新しい名前" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["name"], "新しい名前");
    assert_eq!(patched["institution"], "元の証券会社");
    assert_ne!(patched["updated_at"], created["updated_at"]); // トリガが効いている

    // institution に null → NULL 化
    let (status, patched) = request(
        &app,
        Method::PATCH,
        &path,
        Some(&user.token),
        Some(json!({ "institution": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(patched["institution"].is_null());
    assert_eq!(patched["name"], "新しい名前");

    // 空ボディは 400
    let (status, _) = request(
        &app,
        Method::PATCH,
        &path,
        Some(&user.token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// 他人の口座は 403 ではなく 404（存在自体を漏らさない）
#[sqlx::test(migrations = "./migrations")]
async fn other_users_account_is_not_found(db: PgPool) {
    let app = test_app(db);
    let owner = register_user(&app, "owner@example.com").await;
    let intruder = register_user(&app, "intruder@example.com").await;

    let body = json!({ "name": "他人の口座", "account_type": "ideco" });
    let (_, created) =
        request(&app, Method::POST, "/accounts", Some(&owner.token), Some(body)).await;
    let path = format!("/accounts/{}", created["id"].as_str().unwrap());

    for method in [Method::GET, Method::DELETE] {
        let (status, _) = request(&app, method, &path, Some(&intruder.token), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    let (status, _) = request(
        &app,
        Method::PATCH,
        &path,
        Some(&intruder.token),
        Some(json!({ "name": "乗っ取り" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 侵入者の一覧は空のまま
    let (_, list) = request(&app, Method::GET, "/accounts", Some(&intruder.token), None).await;
    assert!(list.as_array().unwrap().is_empty());
}

/// 削除は 204、2回目は 404
#[sqlx::test(migrations = "./migrations")]
async fn delete_then_gone(db: PgPool) {
    let app = test_app(db);
    let user = register_user(&app, "owner@example.com").await;

    let body = json!({ "name": "消す口座", "account_type": "bank" });
    let (_, created) =
        request(&app, Method::POST, "/accounts", Some(&user.token), Some(body)).await;
    let path = format!("/accounts/{}", created["id"].as_str().unwrap());

    let (status, _) = request(&app, Method::DELETE, &path, Some(&user.token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = request(&app, Method::DELETE, &path, Some(&user.token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = request(&app, Method::GET, &path, Some(&user.token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// 認証なしは 401
#[sqlx::test(migrations = "./migrations")]
async fn requires_authentication(db: PgPool) {
    let app = test_app(db);
    let _ = Uuid::new_v4();

    let (status, _) = request(&app, Method::GET, "/accounts", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}