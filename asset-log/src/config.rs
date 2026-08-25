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
        })
    }
}
