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

// src/lib.rs の末尾に追加
use axum::{routing::{get, post}, Router};
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
        .with_state(state)
}

async fn health() -> &'static str {
    "OK"
}
