use std::sync::Arc;

use crate::config::Config;
use crate::error::AppError;

/// バッチ用トークンの保持と検証。未設定状態も表現できるようにしている
#[derive(Clone)]
pub struct JobToken(Option<Arc<str>>);

impl JobToken {
    pub fn from_config(config: &Config) -> Self {
        Self(config.snapshot_job_token.as_deref().map(Arc::from))
    }

    /// テスト用。任意のトークンで組み立てる
    pub fn new(token: impl AsRef<str>) -> Self {
        Self(Some(Arc::from(token.as_ref())))
    }

    pub fn disabled() -> Self {
        Self(None)
    }

    pub fn verify(&self, presented: &str) -> Result<(), AppError> {
        let expected = self.0.as_deref().ok_or_else(|| {
            AppError::ServiceUnavailable(
                "バッチ実行は無効化されています（SNAPSHOT_JOB_TOKEN 未設定）".into(),
            )
        })?;

        if constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
            Ok(())
        } else {
            Err(AppError::Unauthorized)
        }
    }
}

/// 不一致位置で早期リターンしないバイト比較。
/// 応答時間の差からトークンを1バイトずつ推測されるのを防ぐ。
/// 長さの一致・不一致だけは漏れるが、長さは秘密ではないので許容する。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}
