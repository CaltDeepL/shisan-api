#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_ttl_minutes: i64,
    pub fx_api_base_url: String,   // 既定 https://api.frankfurter.dev/v1
    pub fx_timeout_ms: u64,        // 既定 3000
    pub fx_max_calendar_days: i64, // 既定 4
    pub fx_max_business_days: i64, // 既定 2
    /// バッチ用トークン。未設定なら /snapshots/run を 503 で拒否する
    pub snapshot_job_token: Option<String>,
    /// CORS 許可オリジン。未設定（空）ならブラウザからのリクエストは全て拒否される
    pub cors_allowed_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_secret = std::env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET が設定されていません"))?;
        if jwt_secret.len() < 32 {
            anyhow::bail!(
                "JWT_SECRET は32バイト以上にしてください（現在 {}）",
                jwt_secret.len()
            );
        }
        Ok(Self {
            fx_api_base_url: std::env::var("FX_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.frankfurter.dev/v1".into()),
            fx_timeout_ms: std::env::var("FX_TIMEOUT_MS")
                .unwrap_or_else(|_| "3000".into())
                .parse()?,
            fx_max_calendar_days: std::env::var("FX_MAX_CALENDAR_DAYS")
                .unwrap_or_else(|_| "4".into())
                .parse()?,
            fx_max_business_days: std::env::var("FX_MAX_BUSINESS_DAYS")
                .unwrap_or_else(|_| "2".into())
                .parse()?,
            database_url: std::env::var("DATABASE_URL")?,
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()?,
            jwt_secret,
            jwt_ttl_minutes: std::env::var("JWT_TTL_MINUTES")
                .unwrap_or_else(|_| "60".into())
                .parse()?,
            snapshot_job_token: match std::env::var("SNAPSHOT_JOB_TOKEN") {
                Ok(v) if v.trim().is_empty() => None,
                Ok(v) if v.len() < 32 => anyhow::bail!(
                    "SNAPSHOT_JOB_TOKEN は32バイト以上にしてください（現在 {}）",
                    v.len()
                ),
                Ok(v) => Some(v),
                Err(_) => None,
            },
            cors_allowed_origins: parse_cors_origins(
                &std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default(),
            )?,
        })
    }
}
/// カンマ区切りの許可オリジンをパースする。
/// 空要素は捨て、末尾スラッシュは除去する。
fn parse_cors_origins(raw: &str) -> anyhow::Result<Vec<String>> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if !s.starts_with("http://") && !s.starts_with("https://") {
                anyhow::bail!("CORS_ALLOWED_ORIGINS はスキームを含めてください（不正な値: {s}）");
            }
            Ok(s.trim_end_matches('/').to_string())
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::parse_cors_origins;

    #[test]
    fn empty_string_yields_no_origins() {
        assert!(parse_cors_origins("").unwrap().is_empty());
    }

    #[test]
    fn parses_multiple_origins_with_spaces() {
        assert_eq!(
            parse_cors_origins("http://localhost:5173, https://example.com").unwrap(),
            vec!["http://localhost:5173", "https://example.com"],
        );
    }

    #[test]
    fn strips_trailing_slash_and_blank_entries() {
        assert_eq!(
            parse_cors_origins("http://localhost:5173/,,https://example.com/").unwrap(),
            vec!["http://localhost:5173", "https://example.com"],
        );
    }

    #[test]
    fn rejects_origin_without_scheme() {
        assert!(parse_cors_origins("localhost:5173").is_err());
    }
}
