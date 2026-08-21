use crate::auth::jwt::JwtKeys;
use crate::provider::fx::FxRateProvider;
use axum::extract::FromRef;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt: JwtKeys,
    pub fx: Arc<dyn FxRateProvider>,
}

impl FromRef<AppState> for JwtKeys {
    fn from_ref(s: &AppState) -> Self {
        s.jwt.clone()
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(s: &AppState) -> Self {
        s.db.clone()
    }
}
