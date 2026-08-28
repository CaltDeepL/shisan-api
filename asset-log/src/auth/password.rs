use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;
use std::sync::LazyLock;

/// ユーザー不在時に検証を空回しするためのダミー。
/// 起動時に1回だけ計算する。
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("dummy-password-for-timing-equalization").expect("ダミーハッシュの生成に失敗")
});

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// ユーザーが存在しなかった場合に呼ぶ。常に false を返すが、
/// 実際のハッシュ検証と同じだけ時間を消費する。
pub fn verify_dummy(plain: &str) -> bool {
    verify_password(plain, &DUMMY_HASH);
    false
}

pub fn warmup() {
    LazyLock::force(&DUMMY_HASH);
}
