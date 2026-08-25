use asset_log::auth::{job::JobToken, jwt::JwtKeys};
use asset_log::provider::{
    cached_fx::{CachedFxProvider, StalePolicy},
    fx::FrankfurterClient,
};
use asset_log::{auth, config, state};
use clap::{Parser, Subcommand};
use config::Config;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::{process, sync::Arc, time::Duration};
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
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // DUMMY_HASH の遅延初期化をここで済ませる。
    // 未登録メールでの初回ログインだけ Argon2 が2回走るのを防ぐ。
    tokio::task::spawn_blocking(auth::password::warmup)
        .await
        .expect("warmup task panicked");

    // ホストから直接起動するとき用。コンテナでは compose の environment が使われる
    let _ = dotenvy::dotenv();
    // 設定不備は起動時に落とす（実行中に気づくより安全）
    let config = Config::from_env().expect("設定の読み込みに失敗しました");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let jwt = JwtKeys::new(&config.jwt_secret, config.jwt_ttl_minutes);

    let fx_client = FrankfurterClient::new(
        &config.fx_api_base_url,
        Duration::from_millis(config.fx_timeout_ms),
    )
    .expect("failed to build FX HTTP client");

    let fx = Arc::new(CachedFxProvider::new(
        fx_client,
        pool.clone(),
        StalePolicy {
            max_calendar_days: config.fx_max_calendar_days,
            max_business_days: config.fx_max_business_days,
        },
    ));

    let state = AppState {
        db: pool,
        jwt,
        fx,
        job_token: JobToken::from_config(&config),
    };

    let app = asset_log::app(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port))
        .await
        .expect("failed to bind port");

    tracing::info!("listening on port {}", config.port);

    axum::serve(listener, app).await.expect("server error");
}
