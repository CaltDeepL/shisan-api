pub mod auth;
pub mod config;
pub mod domain;
pub mod error;
pub mod handler;
pub mod job;
pub mod middleware;
pub mod openapi;
pub mod provider;
pub mod repository;
pub mod service;
pub mod state;

use axum::Router;
use state::AppState;

use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;

pub fn app(state: AppState) -> Router {
    let (documented, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(handler::health::health))
        .routes(routes!(handler::auth::register))
        .routes(routes!(handler::auth::login))
        .routes(routes!(handler::auth::me))
        .routes(routes!(handler::accounts::create, handler::accounts::list))
        .routes(routes!(
            handler::assets::create_asset,
            handler::assets::list_assets
        ))
        .routes(routes!(
            handler::assets::get_asset,
            handler::assets::patch_asset
        ))
        .routes(routes!(handler::prices::upsert_prices))
        .routes(routes!(handler::prices::get_price_history))
        .routes(routes!(
            handler::accounts::get,
            handler::accounts::update,
            handler::accounts::delete
        ))
        .routes(routes!(
            handler::transactions::create,
            handler::transactions::list
        ))
        .routes(routes!(
            handler::transactions::show,
            handler::transactions::delete
        ))
        .routes(routes!(handler::holdings::list))
        .routes(routes!(handler::analytics::asset_history))
        .routes(routes!(handler::analytics::allocation))
        .routes(routes!(handler::snapshots::run))
        .routes(routes!(handler::import::create))
        .routes(routes!(handler::import::dry_run))
        .routes(routes!(handler::fx::get_rate))
        .split_for_parts();

    documented
        .with_state(state)
        .merge(SwaggerUi::new("/docs").url("/openapi.json", api))
}
