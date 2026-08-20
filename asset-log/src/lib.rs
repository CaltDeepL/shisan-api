pub mod auth;
pub mod config;
pub mod domain;
pub mod error;
pub mod handler;
pub mod job;
pub mod middleware;
pub mod provider;
pub mod repository;
pub mod service;
pub mod state;

use axum::{
    Router,
    routing::{get, post},
};
use state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(handler::auth::register))
        .route("/auth/login", post(handler::auth::login))
        .route("/me", get(handler::auth::me))
        .route(
            "/accounts",
            get(handler::accounts::list).post(handler::accounts::create),
        )
        .route(
            "/accounts/{id}",
            get(handler::accounts::get)
                .patch(handler::accounts::update)
                .delete(handler::accounts::delete),
        )
        .route(
            "/assets",
            get(handler::assets::list_assets).post(handler::assets::create_asset),
        )
        .route(
            "/assets/{id}",
            get(handler::assets::get_asset).patch(handler::assets::patch_asset),
        )
        .route("/prices", post(handler::prices::upsert_prices))
        .route(
            "/prices/{asset_id}",
            get(handler::prices::get_price_history),
        )
        .with_state(state)
}

async fn health() -> &'static str {
    "OK"
}
