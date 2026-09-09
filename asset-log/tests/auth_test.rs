mod common;

use asset_log::auth::{
    jwt::{Claims, JwtKeys},
    password::{hash_password, verify_password},
};
use axum::http::{Method, StatusCode};
use chrono::Utc;
use common::{request, test_app};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

// argon2 0.5 系と同じ Argon2id v19 / m=19456,t=2,p=1 の PHC fixture。
// 依存更新後も既存ユーザーの保存済み PHC 文字列を検証できることを固定する。
const LEGACY_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$bGVnYWN5LXNhbHQtMTIzNA$S/VTkxuGTEjfn2OCCWzgQxE84hQNcD+UKQ630V8+Jbs";

fn encode_claims(secret: &str, algorithm: Algorithm, claims: &Claims) -> String {
    encode(
        &Header::new(algorithm),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("token encoding should succeed")
}

fn tamper_signature(token: &str) -> String {
    let (prefix, signature) = token.rsplit_once('.').expect("JWT has three segments");
    let mut bytes = signature.as_bytes().to_vec();
    let first = bytes.first_mut().expect("signature is not empty");
    *first = if *first == b'A' { b'B' } else { b'A' };
    format!(
        "{prefix}.{}",
        String::from_utf8(bytes).expect("base64url is ASCII")
    )
}

#[test]
fn password_hash_and_verify_round_trip() {
    let hash = hash_password("correct-horse-battery-staple").expect("hashing should succeed");

    assert!(hash.starts_with("$argon2id$v=19$"));
    assert!(verify_password("correct-horse-battery-staple", &hash));
    assert!(!verify_password("wrong-password", &hash));
}

#[test]
fn password_uses_random_salt() {
    let first = hash_password("same-password-each-time").expect("first hash");
    let second = hash_password("same-password-each-time").expect("second hash");

    assert_ne!(first, second);
    assert!(verify_password("same-password-each-time", &first));
    assert!(verify_password("same-password-each-time", &second));
}

#[test]
fn password_verifies_legacy_phc_hash() {
    assert!(verify_password("legacy-password", LEGACY_PHC));
    assert!(!verify_password("wrong-password", LEGACY_PHC));
}

#[test]
fn password_rejects_malformed_hash() {
    assert!(!verify_password("password", "not-a-phc-string"));
}

#[test]
fn jwt_issue_and_verify_round_trip() {
    let keys = JwtKeys::new("unit-test-secret-for-hs256", 60);
    let user_id = Uuid::new_v4();

    let (token, expires_in) = keys.issue(user_id).expect("JWT issue should succeed");
    let claims = keys.verify(&token).expect("issued JWT should verify");

    assert_eq!(expires_in, 60 * 60);
    assert_eq!(claims.sub, user_id);
    assert!(claims.exp > claims.iat);
}

#[test]
fn jwt_rejects_expired_token_without_sleeping() {
    let secret = "unit-test-secret-for-hs256";
    let keys = JwtKeys::new(secret, 60);
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: Uuid::new_v4(),
        iat: now - 120,
        exp: now - 60,
    };
    let token = encode_claims(secret, Algorithm::HS256, &claims);

    assert!(keys.verify(&token).is_none());
}

#[test]
fn jwt_rejects_tampered_token() {
    let keys = JwtKeys::new("unit-test-secret-for-hs256", 60);
    let (token, _) = keys.issue(Uuid::new_v4()).expect("issue");
    let tampered = tamper_signature(&token);

    assert!(keys.verify(&tampered).is_none());
}

#[test]
fn jwt_rejects_token_signed_with_another_secret() {
    let issuer = JwtKeys::new("issuer-secret-for-hs256", 60);
    let verifier = JwtKeys::new("different-verifier-secret", 60);
    let (token, _) = issuer.issue(Uuid::new_v4()).expect("issue");

    assert!(verifier.verify(&token).is_none());
}

#[test]
fn jwt_rejects_non_hs256_algorithm() {
    let secret = "unit-test-secret-for-hs256";
    let keys = JwtKeys::new(secret, 60);
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: Uuid::new_v4(),
        iat: now,
        exp: now + 3600,
    };
    let token = encode_claims(secret, Algorithm::HS384, &claims);

    assert!(keys.verify(&token).is_none());
}

#[sqlx::test]
async fn register_counts_unicode_characters_not_bytes(db: PgPool) {
    let app = test_app(db);
    let body = json!({
        "email": "unicode-password@example.com",
        "password": "安全安全安全安全安全安全"
    });

    let (status, response) = request(&app, Method::POST, "/auth/register", None, Some(body)).await;

    assert_eq!(status, StatusCode::CREATED, "{response}");
}

#[sqlx::test]
async fn register_rejects_password_shorter_than_12_characters(db: PgPool) {
    let app = test_app(db);
    let body = json!({
        "email": "short-password@example.com",
        "password": "abcdefghijk"
    });

    let (status, response) = request(&app, Method::POST, "/auth/register", None, Some(body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["errors"][0]["field"], "password");
}

#[sqlx::test]
async fn register_rejects_password_over_256_characters(db: PgPool) {
    let app = test_app(db);
    let body = json!({
        "email": "long-password@example.com",
        "password": "a".repeat(257)
    });

    let (status, response) = request(&app, Method::POST, "/auth/register", None, Some(body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["errors"][0]["field"], "password");
}

#[sqlx::test]
async fn register_rejects_password_over_512_utf8_bytes(db: PgPool) {
    let app = test_app(db);
    let body = json!({
        "email": "large-utf8-password@example.com",
        "password": "😀".repeat(200)
    });

    let (status, response) = request(&app, Method::POST, "/auth/register", None, Some(body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["errors"][0]["field"], "password");
}

#[sqlx::test]
async fn register_rejects_common_password(db: PgPool) {
    let app = test_app(db);
    let body = json!({
        "email": "common-password@example.com",
        "password": "password1234"
    });

    let (status, response) = request(&app, Method::POST, "/auth/register", None, Some(body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["errors"][0]["field"], "password");
}
