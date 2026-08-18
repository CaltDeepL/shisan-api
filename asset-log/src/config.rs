#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_ttl_minutes: i64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_secret = std::env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET が設定されていません"))?;
        if jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET は32バイト以上にしてください（現在 {}）", jwt_secret.len());
        }
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            port: std::env::var("PORT").unwrap_or_else(|_| "8080".into()).parse()?,
            jwt_secret,
            jwt_ttl_minutes: std::env::var("JWT_TTL_MINUTES")
                .unwrap_or_else(|_| "60".into()).parse()?,
        })
    }
}