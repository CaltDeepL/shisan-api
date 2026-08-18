Router::new()
    .route("/health", get(handler::health::health))
    .route("/auth/register", post(handler::auth::register))
    .route("/auth/login", post(handler::auth::login))
    .route("/me", get(handler::auth::me))
    .with_state(state)
    pub mod auth;
pub mod config;
pub mod domain;
pub mod error;
pub mod handler;
pub mod middleware;
pub mod repository;
pub mod state;