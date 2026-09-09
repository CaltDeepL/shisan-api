use asset_log::auth::{
    jwt::JwtKeys,
    password::{hash_password, verify_password},
};
use uuid::Uuid;

// argon2 0.5 系と同じ Argon2id v19 / m=19456,t=2,p=1 の PHC fixture。
// 依存更新後も既存ユーザーの保存済み PHC 文字列を検証できることを固定する。
const LEGACY_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$bGVnYWN5LXNhbHQtMTIzNA$S/VTkxuGTEjfn2OCCWzgQxE84hQNcD+UKQ630V8+Jbs";

#[test]
fn password_hash_and_verify_round_trip() {
    let hash = hash_password("correct-horse-battery-staple").expect("hashing should succeed");

    assert!(hash.starts_with("$argon2id$v=19$"));
    assert!(verify_password("correct-horse-battery-staple", &hash));
    assert!(!verify_password("wrong-password", &hash));
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
