use axum::extract::FromRef;
use sqlx::PgPool;
use crate::auth::jwt::JwtKeys;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt: JwtKeys,
}

impl FromRef<AppState> for JwtKeys {
    fn from_ref(s: &AppState) -> Self { s.jwt.clone() }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(s: &AppState) -> Self { s.db.clone() }
}