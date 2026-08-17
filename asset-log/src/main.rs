mod config;
mod error;
mod state;

use axum::{routing::get, Router};
use clap::{Parser, Subcommand};
use config::Config;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::process;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "asset-log")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// コンテナのヘルスチェック用サブコマンド
    Healthcheck,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Healthcheck) => run_healthcheck(),
        None => {
            let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
            rt.block_on(run_server());
        }
    }
}

fn run_healthcheck() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let url = format!("http://127.0.0.1:{port}/health");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    match client.get(&url).send() {
        Ok(response) if response.status().is_success() => {
            println!("Healthcheck passed.");
            process::exit(0);
        }
        _ => {
            eprintln!("Healthcheck failed.");
            process::exit(1);
        }
    }
}

async fn run_server() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let config = Config::from_env();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let state = AppState { db: pool };

    let app = Router::new()
        .route("/health", get(health_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port))
        .await
        .expect("failed to bind port");

    tracing::info!("listening on port {}", config.port);

    axum::serve(listener, app)
        .await
        .expect("server error");
}

async fn health_handler() -> &'static str {
    "OK"
}