use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Clone)]
pub struct JwtKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    ttl_minutes: i64,
}

impl JwtKeys {
    pub fn new(secret: &str, ttl_minutes: i64) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5; // 時刻ずれの許容（秒）
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            validation,
            ttl_minutes,
        }
    }

    pub fn issue(&self, user_id: Uuid) -> anyhow::Result<(String, i64)> {
        let now = Utc::now().timestamp();
        let exp = now + self.ttl_minutes * 60;
        let claims = Claims { sub: user_id, iat: now, exp };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)?;
        Ok((token, self.ttl_minutes * 60))
    }

    pub fn verify(&self, token: &str) -> Option<Claims> {
        decode::<Claims>(token, &self.decoding, &self.validation)
            .ok()
            .map(|d| d.claims)
    }
}