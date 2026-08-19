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

pub fn app(state: AppState) -> Router { ... }