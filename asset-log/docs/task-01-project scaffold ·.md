# タスク#1 作業メモ: プロジェクト雛形・/health・compose

- **対象**: 資産管理API（asset-log）
- **完了日**: 2026-08-18
- **成果物**: `src/main.rs`, `src/config.rs`, `src/state.rs`, `src/error.rs`, `Dockerfile`, `compose.yaml`, `.env.example`

---

## 1. ゴールと完了条件

| # | 完了条件 | 結果 |
|---|---|---|
| 1 | `docker compose up --build -d` で db / api が両方 `healthy` になる | OK |
| 2 | `curl localhost:8080/health` が 200 を返す | OK |
| 3 | distroless実行イメージでヘルスチェックが機能する | OK |
| 4 | `.env` を使ってホスト側・コンテナ側どちらの起動でも環境変数が解決できる | OK |

---

## 2. 構築した構成

### 2.1 Docker（マルチステージビルド）

```dockerfile
# ---- builder ----
FROM rust:1.96-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release

COPY . .
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    touch src/main.rs && \
    cargo build --release && \
    cp /app/target/release/asset-log /app/asset-log

# ---- runtime ----
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=builder --chown=nonroot:nonroot /app/asset-log .
COPY --from=builder --chown=nonroot:nonroot /app/migrations ./migrations

EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["./asset-log", "healthcheck"]
CMD ["./asset-log"]
```

- builder: `rust:1.96-slim`。ダミー`main.rs`で依存クレートだけ先にビルドし、
  BuildKitキャッシュマウント（`/usr/local/cargo/registry`, `/app/target`）で
  再ビルドを高速化する
- runtime: `gcr.io/distroless/cc-debian12:nonroot`。シェル・パッケージマネージャを
  含まない最小イメージで攻撃対象領域を削減し、非rootユーザーで実行する
- バイナリ名は `Cargo.toml` の `[package] name` と一致させる（`asset-log`）

### 2.2 compose.yaml

```yaml
services:
  db:
    image: postgres:17-alpine
    environment:
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: ${POSTGRES_DB}
      TZ: Asia/Tokyo
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U assetlog -d assetlog"]
      interval: 5s
      timeout: 3s
      retries: 10

  api:
    build:
      context: ./asset-log
    environment:
      DATABASE_URL: ${DATABASE_URL}
      PORT: ${PORT}
      RUST_LOG: ${RUST_LOG}
    ports:
      - "8080:8080"
    depends_on:
      db:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "./asset-log", "healthcheck"]
      interval: 10s
      timeout: 3s
      retries: 5

volumes:
  pgdata:
```

- 環境変数はハードコードをやめ、`.env` から `${VAR}` 展開する方式に統一
- `depends_on: condition: service_healthy` で db 起動完了を待ってから api を起動
- `.env` は `compose.yaml` と同階層（リポジトリルート）に置く必要がある。
  `asset-log/.env` に置くと Compose から認識されず変数が空扱いになる

### 2.3 アプリケーション本体

`main.rs` — clap で CLI 分岐し、通常起動時は tokio runtime を手動生成して
axum サーバーを立ち上げる。`healthcheck` サブコマンドは reqwest blocking で
自身の `/health` を叩き、成否を exit code で返す。

```rust
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
    axum::serve(listener, app).await.expect("server error");
}

async fn health_handler() -> &'static str {
    "OK"
}
```

`config.rs` — 環境変数読み込み。

```rust
#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .expect("PORT must be a valid number");
        Self { database_url, port }
    }
}
```

`state.rs` — アプリ共有状態。

```rust
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}
```

`error.rs` — 後続タスクで使うエラーハンドリングの土台
（RFC 9457 Problem Details準拠）。現時点では `health_handler` が
`Result` を返さないため未使用警告が出るが、口座CRUD以降で使用する。

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("internal server error")]
    Internal(#[from] anyhow::Error),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, title) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not Found"),
            AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
            }
        };
        if matches!(self, AppError::Database(_) | AppError::Internal(_)) {
            tracing::error!(error = ?self, "request failed");
        }
        let body = Json(json!({
            "type": "about:blank",
            "title": title,
            "status": status.as_u16(),
        }));
        (status, body).into_response()
    }
}
```

---

## 3. 設計判断とその理由

### 3.1 実行イメージに distroless を採用

**採用案**: `gcr.io/distroless/cc-debian12:nonroot`
**却下案**: `debian:bookworm-slim`

理由:
- シェル・パッケージマネージャを含まないため、攻撃対象領域が小さい
- 非rootユーザー（`nonroot:nonroot`）がデフォルトで用意されており
  権限分離の実装コストがゼロ
- 代償として `curl` 等の汎用コマンドが使えなくなるため、ヘルスチェックの
  実装方式を変える必要があった（3.2参照）

### 3.2 ヘルスチェックを自前バイナリのサブコマンドにした

**採用案**: `clap` で `healthcheck` サブコマンドを実装し、
`reqwest::blocking` で自身の `/health` を叩く
**却下案**: `curl -f http://localhost:8080/health`

理由:
- distroless に `curl` が存在せず `exec: "curl": executable file not found`
  で確実に失敗する
- Rustバイナリ自身に持たせれば、distrolessを維持したままヘルスチェックが
  完結する
- 副作用として、`main()` を同期関数のまま保ち、サーバー起動時のみ
  `tokio::runtime::Runtime` を手動生成する構成になった
  （ヘルスチェック用の軽量な同期パスと、本体の非同期サーバーを分離するため）

### 3.3 環境変数を `.env` + `${VAR}` 展開に統一

**採用案**: `compose.yaml` はハードコードせず `${DATABASE_URL}` 等で参照し、
`.env` を別途用意
**却下案**: `compose.yaml` に直接値を書き込む

理由:
- パスワード等の値を変更する際に更新箇所が1箇所で済む
- `.env.example` をリポジトリにコミットし、`.env` は `.gitignore` する
  運用と相性が良い

### 3.4 マイグレーションをアプリ起動時に自動実行する

**採用案**: `run_server()` 内で `sqlx::migrate!("./migrations").run(&pool)`
を呼ぶ

理由:
- `docker compose up` だけでスキーマまで含めて再現できることを
  非機能要件（開発のしやすさ）として設計時点で定めていたため
- Dockerfile で `migrations/` ディレクトリを実行イメージにコピーしておく
  必要がある（`COPY --from=builder ... /app/migrations ./migrations`）

---

## 4. つまずいた点と原因

### 4.1 distroless イメージ上で `apt-get` を実行しようとした

**症状**: `RUN apt-get update && apt-get install -y ca-certificates curl`
が実行イメージ側で失敗する。

**原因**: distroless にはパッケージマネージャが存在しない。debian-slim版の
Dockerfileをそのままdistroless版にコピーしてしまったのが原因。

**教訓**: マルチステージの2段目を差し替えたときは、1段目由来の
`RUN apt-get` 系の行が紛れ込んでいないか必ず見直す。

### 4.2 `CMD` が参照するバイナリ名の不一致

**症状**: `cp: cannot stat '/app/target/release/my-rust-app': No such file or directory`
でビルドが失敗。

**原因**: サンプルコードの汎用名 `my-rust-app` をそのまま使ってしまい、
実際の `Cargo.toml` の `[package] name`（`asset-log`）と一致していなかった。

**教訓**: サンプル・テンプレートのバイナリ名は、貼り付ける前に必ず
実プロジェクトの `Cargo.toml` と突き合わせる。`cargo build` のログに
出る `Compiling <name> v0.1.0` の `<name>` が正なので、そこを起点に
Dockerfile・compose.yaml・`main.rs` の `#[command(name = ...)]` を揃える。

### 4.3 `ENV SQLX_OFFLINE=true` の配置ミスと無駄な3回目ビルド

**症状**: 本ビルドより後に `ENV SQLX_OFFLINE=true` が置かれており、
さらにそのあとにキャッシュマウントなしの3回目の `cargo build` が
残っていた（成果物はどこにもコピーされず捨てられる）。

**原因**: 複数バージョンのDockerfile断片をマージした際に、
古い断片の末尾を消し忘れた。

**教訓**: `ENV` はそれを必要とする `RUN` より必ず前に置く。
複数の下書きを合成するときは、最終的に1本のビルドパスしか
残らないことを行ごとに確認する。

### 4.4 `services.test must be a mapping`

**症状**: `docker compose up` 実行時に構文エラー。

**原因**: `healthcheck.test` のインデントが崩れ、`test:` が
`services:` 直下として解釈され「`test` という名前のサービス」扱いに
なっていた。

**教訓**: YAMLはインデント崩れがそのまま別の構造として解釈されるため、
エディタの自動整形に頼らず、`api:` → `healthcheck:` → `test:` の
階層をインデント幅で目視確認する。

### 4.5 `test: [["CMD", "./asset-log", "healthcheck"]]`（角括弧の二重ネスト）

**症状**: `services.api.healthcheck.test.0 must be a string`。

**原因**: 配列を1つだけ含む配列になってしまい、`test[0]` が文字列ではなく
配列そのものになっていた。

**教訓**: `test:` の値は「コマンドとその引数を並べたフラットな配列」
1つだけ。`[ ["CMD", ...] ]` のように外側にもう一段 `[ ]` を足さない。

### 4.6 `db` と `api` の healthcheck 定義が入れ替わった

**症状**: `db` に `./asset-log healthcheck`、`api` に `pg_isready` が
設定されており、両方とも必ず失敗する状態になっていた。

**原因**: 複数回の編集を重ねる中で、コピー＆ペースト時にサービス単位の
対応関係を崩してしまった。

**教訓**: サービスごとに異なるヘルスチェック手段を使う構成では、
編集後に必ず「どのサービスにどのコマンドが対応しているか」を
サービス名と突き合わせて確認する。

### 4.7 `POSTGRES_PASSWORD` のタイポ（`pOSTGRES_PASSWORD`）

**症状**: DBコンテナがパスワード未設定として起動する。

**原因**: 環境変数キーの先頭が誤って小文字になっていた。

**教訓**: 環境変数名は大文字小文字を区別する。公式イメージが要求する
キー名（`POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB`）は
コピー元のドキュメントと完全一致させる。

### 4.8 `.env` の配置ディレクトリ違い

**症状**: `docker compose up` 実行時に
`WARN: The "DATABASE_URL" variable is not set. Defaulting to a blank string.`
が複数出力される。

**原因**: `.env` が `asset-log/`（Rustクレート側）に置かれており、
`compose.yaml` があるリポジトリルートから見つからなかった。
Composeはカレントディレクトリ（＝`compose.yaml`のあるディレクトリ）の
`.env` しか自動で読まない。

**教訓**: `.env` は必ず `compose.yaml` と同階層に置く。クレート側に
別途 `.env` が必要な操作（`sqlx` CLI 等）がある場合は、変数名を
分けるか `--env-file` を明示する。

### 4.9 `main.rs` の実サーバー起動処理が実装されていなかった

**症状**: `docker compose ps -a` で `api` コンテナが
`Exited (0)`。ログは `Starting web server on port 8080...` の
1行のみで、それ以上何も起きずプロセスが正常終了していた。

**原因**: ヘルスチェック実装のサンプルコードをそのまま `main.rs` として
使っており、`None =>` 分岐の中身（実際のサーバー起動処理）が
`// start_server();` というコメントのままだった。当初「元の実装を
壊してしまった」と誤認したが、実際にはこの時点で `main.rs` に
本実装が一度も書かれていなかったことが後で判明した。

**教訓**: exit code 0 での即終了は「エラーではなく何もしていない」
サインであることが多い。ログに異常が出ていない場合こそ、
処理本体が本当に実装されているか（コメントアウトのまま
放置されていないか）をまず疑う。

---

## 5. 次タスクへの引き継ぎ

- タスク#2（マイグレーション0001: users/accounts）は、本タスクで
  `main.rs` に組み込んだ `sqlx::migrate!("./migrations")` が
  そのまま実行経路になる。マイグレーションファイルを
  `migrations/` に追加するだけで、コンテナ再起動時に自動適用される
- `error.rs` の `AppError` は現状 `NotFound` / `Database` / `Internal`
  の3バリアントのみ。タスク#3（AppError整備）で、Postgresの
  制約違反エラーコード（`23505` 等）を個別バリアントに
  マッピングする拡張が必要
- ヘルスチェック方式（自前バイナリの `healthcheck` サブコマンド）は
  Dockerfile・compose.yaml両方に前提として組み込まれているため、
  今後 `/health` のパスやポートの扱いを変える場合は
  `run_healthcheck()` 側も合わせて修正すること

---

## 6. 実行コマンド一覧（再現用）

```bash
cd ~/workspace/shisan-api

cp asset-log/.env.example .env   # .env は compose.yaml と同階層に置く

docker compose up --build -d
docker compose ps
docker compose logs api

curl localhost:8080/health
```