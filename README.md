# shisan-api

[![CI](https://github.com/CaltDeepL/shisan-api/actions/workflows/ci.yml/badge.svg)](https://github.com/CaltDeepL/shisan-api/actions/workflows/ci.yml)

NISA・iDeCo を含む複数口座の資産を横断的に管理し、損益と収益率を可視化する資産管理 API です。

証券会社ごとにアプリを開いて残高を確認する手間をなくし、「制度をまたいだ資産全体で、実際にいくら増えたのか」を一箇所で把握することを目的としています。

**デモ**: https://shisan-api.onrender.com/docs

Swagger UI からブラウザ上で全エンドポイントを試せます。`POST /auth/register` でアカウントを作成し、返却されたトークンを右上の **Authorize** に入力してください。

> 無料プランで稼働しているため、アクセスがない間はインスタンスが停止します。最初のリクエストは応答まで数十秒かかることがあります。

> **ポートフォリオプロジェクトです。** 全16タスクのロードマップを完了し、CI・自動デプロイ・日次バッチが稼働しています。次は XIRR（金額加重収益率）と Google ログイン（OIDC）の追加を予定しています。詳細は[実装状況](#実装状況)を参照してください。

---

## なぜ作ったか

既存の家計簿アプリでも残高の合算はできますが、以下が扱いにくいと感じました。

- **NISA の非課税枠が制度どおりに管理されない** — 2024年以降の新NISAは「つみたて投資枠」と「成長投資枠」で年間上限が異なるため、枠を区別せずに集計すると意味をなさない
- **収益率が単純な損益率でしか出ない** — 積立のように入金タイミングがばらつく場合、単純な損益率では実質的なパフォーマンスがわからない。金額加重収益率（XIRR）が必要（16タスク完了後の追加機能として実装予定）
- **税引後のリターンが見えない** — 特定口座と非課税口座が混在していると、額面の損益と手元に残る額が乖離する

これらを扱うには、口座種別と取引履歴を制度に即した形でモデリングする必要があります。その設計自体がこのプロジェクトの主題です。

---

## 技術スタック

| 領域 | 技術 | 選定理由 |
|---|---|---|
| バックエンド | Rust 1.96 / axum 0.8 | 金額計算で型安全性を活かしたい。`Option<T>` と ENUM で「ありえない状態」をコンパイル時に排除する |
| DB | PostgreSQL 17 | ENUM 型、関数インデックス、CHECK 制約でドメイン制約を DB 層でも担保する |
| DB アクセス | sqlx 0.9 | コンパイル時に SQL を検証できる。ORM を使わず SQL を書く方針 |
| フロントエンド | Vite + React + TypeScript | — |
| コンテナ | Docker / Docker Compose | マルチステージビルド + distroless |
| 外部 API | Frankfurter（為替） | 認証不要で ECB のレートを取得できる |
| ホスティング | Render（Docker） | 無料枠で Docker イメージをそのまま動かせる |
| 本番 DB | Neon（Postgres 17） | 無料枠に期限がなく、接続プーリングを備える |
| CI / CD | GitHub Actions | テスト・デプロイ・日次バッチを一箇所で管理できる |

---

## アーキテクチャ

レイヤードアーキテクチャを採用し、計算ロジックを I/O から分離しています。

```
handler    HTTP の入出力のみ。リクエスト検証とレスポンス整形
   ↓
service    ユースケースの組み立て（holdings_service / analytics_service / allocation_service）
   ↓
repository DB アクセス。SQL はここに閉じる
   ↓
domain     純粋関数。総平均法による取得単価、評価損益
```

`provider` 層で為替レートと株価の取得を trait として抽象化し、テスト時にモックへ差し替えられるようにしています。

### ディレクトリ構成

```
shisan-api/
├── compose.yaml              # Postgres + API
├── .env                      # Compose 用の環境変数（gitignore 対象）
├── .github/workflows/        # CI / Deploy / Daily Snapshot
├── src/                      # フロントエンド（Vite + React）
└── asset-log/                # バックエンド（Rust）
    ├── Dockerfile
    ├── .env                  # sqlx CLI 用の DATABASE_URL（gitignore 対象）
    ├── .sqlx/                # オフラインクエリキャッシュ
    ├── migrations/           # sqlx マイグレーション
    ├── docs/                 # タスクごとの設計メモ / openapi.json
    ├── tests/                # 統合テスト
    └── src/
        ├── main.rs           # clap CLI + axum 起動
        ├── lib.rs            # ルータ組み立て（統合テストからも参照）
        ├── config.rs         # 環境変数
        ├── state.rs          # AppState / PgPool
        ├── error.rs          # AppError → IntoResponse（RFC 9457）
        ├── openapi.rs        # OpenAPI 定義（ApiDoc / セキュリティスキーム）
        ├── domain/           # position / money / account_type
        ├── handler/
        ├── service/
        ├── repository/
        ├── provider/         # fx / price（trait）
        ├── middleware/       # auth
        └── job/              # daily_snapshot
```

---

## データモデル

### 口座種別（`account_type` ENUM）

| 値 | 意味 | 課税 |
|---|---|---|
| `tokutei` | 特定口座 | 課税 |
| `ippan` | 一般口座 | 課税 |
| `nisa_tsumitate` | NISA つみたて投資枠 | 非課税 |
| `nisa_growth` | NISA 成長投資枠 | 非課税 |
| `ideco` | iDeCo | 非課税 |
| `bank` | 待機資金（現金） | — |

NISA を2つの値に分けているのは、非課税枠の消費状況を枠ごとに集計する必要があるためです。

特定口座の源泉徴収区分は ENUM を分割せず `accounts.withholding BOOLEAN` で保持しています。この列は **nullable + CHECK 制約**で「特定口座なら必須、それ以外なら NULL」を DB レベルで強制しています。`NOT NULL DEFAULT false` にすると iDeCo 口座の `withholding = false` が「源泉徴収なしの特定口座」と区別できなくなるためです。Rust 側では `Option<bool>` として受け取り、型が意図を語る形にしています。

設計判断の詳細は [`asset-log/docs/`](asset-log/docs/) にタスクごとのメモとして残しています。

---

## セットアップ

### 必要なもの

- Docker / Docker Compose
- Rust 1.96 以降（ローカルでビルドする場合）
- sqlx-cli（マイグレーションを実行する場合）

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
```

### 起動

```bash
git clone https://github.com/CaltDeepL/shisan-api.git
cd shisan-api
cp .env.example .env
```

`.env` の `JWT_SECRET` はサンプル値のままだと32バイト未満で起動時にエラーになります。以下で生成した値に置き換えてください（`POSTGRES_PASSWORD` 等はローカル専用なのでサンプル値のままで構いません）。

```bash
openssl rand -base64 32
```

```bash
docker compose up --build -d
curl http://localhost:8080/health
```

### API ドキュメント

起動後、Swagger UI から全エンドポイントの仕様を確認し、その場でリクエストを試せます。

http://localhost:8080/docs

OpenAPI 3.1 の仕様は `/openapi.json` で配信しており、[`asset-log/docs/openapi.json`](asset-log/docs/openapi.json) にもコミットしています。

### マイグレーション

sqlx CLI はホスト側で実行します。接続先は `asset-log/.env` の `DATABASE_URL` から自動的に読まれるため、引数での指定は不要です（`asset-log/.env` は Compose 用のルート `.env` とは別ファイルなので、初回は個別に作成してください）。

```bash
cd asset-log
cp .env.example .env    # 値を編集（DATABASE_URL のユーザー・パスワードはルートの .env と揃える）
sqlx migrate run
sqlx migrate info
```

> **`.env` が2つある理由**
>
> 接続先のホスト名が実行主体によって異なるため、用途ごとにファイルを分けています。
>
> | ファイル | 用途 | ホスト名 |
> |---|---|---|
> | `asset-log/.env` | ホストのシェル → db コンテナ（sqlx CLI・`cargo test`） | `localhost:5432` |
> | `shisan-api/.env` | api コンテナ → db コンテナ（Compose の変数展開） | `db:5432` |
>
> `db` は Compose ネットワーク内部のサービス名なので、ホストのシェルからは解決できません。逆に、ホスト用の `DATABASE_URL` をシェルで `export` すると、Compose の変数展開がシェルの環境変数を優先するため、API コンテナが自分自身の 5432 を見に行って起動に失敗します。

---

## テスト

```bash
cd asset-log
cargo test --all-targets
```

統合テストは `#[sqlx::test(migrations = "./migrations")]` により、**テストごとに独立した一時 DB** を作成してマイグレーションを適用します。テスト間の状態共有がないため、並列実行しても干渉しません。

`tower::ServiceExt::oneshot` でルータへ直接リクエストを投げる方式を採り、HTTP サーバを起動せずにハンドラからリポジトリまでを通しで検証しています。

外部 API（Frankfurter）は `wiremock` でスタブ化し、正常系に加えて 5xx 応答・タイムアウト時のキャッシュフォールバックまでテストしています。

---

## 実装上の工夫

### OpenAPI 仕様の自動生成

`utoipa-axum` の `OpenApiRouter` を使い、ルート登録とドキュメント生成を同じ場所にまとめています。

```rust
OpenApiRouter::with_openapi(ApiDoc::openapi())
    .routes(routes!(handler::accounts::create, handler::accounts::list))
    .split_for_parts()
```

`routes!()` に渡したハンドラがそのまま axum のルートになり、同時に `#[utoipa::path]` の情報から仕様が組み立てられます。ルートを追加したのにドキュメントを書き忘れる、パスを変更したのに仕様が古いまま、といった乖離が構造的に起きません。

統合テストでもパス数の完全一致を検証しており、エンドポイントを追加すると仕様の更新を促してテストが落ちるようにしています。

### distroless でのヘルスチェック

ランタイムイメージに `gcr.io/distroless/cc-debian12:nonroot` を使っているため、`curl` もシェルも存在しません。そこで clap で `healthcheck` サブコマンドを実装し、バイナリ自身が `/health` を叩く方式に統一しました。Dockerfile の `HEALTHCHECK` 命令と compose.yaml の双方が同じコマンドを呼びます。

```dockerfile
HEALTHCHECK CMD ["./asset-log", "healthcheck"]
```

reqwest の `blocking` フィーチャーは有効化せず、current-thread のランタイムを起こして非同期クライアントを使っています。TLS も `default-features = false` + `rustls-tls` として OpenSSL への依存を持たず、sqlx と同一の rustls / ring を共有しています。

### エラーレスポンス

`AppError` を `IntoResponse` に実装し、RFC 9457（Problem Details for HTTP APIs）準拠の JSON を返します。Postgres のエラーコードを HTTP ステータスにマッピングする方針です。

| コード | 意味 | HTTP |
|---|---|---|
| `23514` | CHECK 制約違反 | 422 |
| `23505` | UNIQUE 制約違反 | 409 |
| `23503` | 外部キー違反 | 404 / 422 |

制約名とメッセージの対応表を持っており、どのフィールドが原因かをクライアントに返します。5xx では内部情報を露出させず、`trace_id` のみを返します。

### 日次スナップショット

資産推移の算出は取引履歴からの再計算を正本としつつ、日次バッチで結果をキャッシュしています。取引が追加・削除された場合は影響する日以降のキャッシュを失効させ、次回参照時に再計算されます。レスポンスの `source` フィールドで、キャッシュとフォールバックのどちらを経由したかが分かります。

「未計算」と「保有ゼロ」を区別するため、計算済みマーカーを別テーブル（`snapshot_days`）に分離しています。

---

## CI / CD

```
push → CI（fmt / clippy / test / .sqlx 検証）
         ↓ 成功時のみ
       Deploy（Render の Deploy Hook を起動）
```

Render の auto-deploy は無効化し、**CI が green のときだけ**デプロイが走る構成にしています。GitHub Actions の `workflow_run` イベントで CI の完了と結果を受け取り、`main` ブランチかつ成功時に限って Deploy Hook を叩きます。

| ワークフロー | トリガー | 内容 |
|---|---|---|
| CI | push / pull_request | `cargo fmt --check`、`clippy -D warnings`、全93テスト、`sqlx prepare --check` |
| Deploy | CI の成功（main のみ） | Render の Deploy Hook を起動 |
| Daily Snapshot | cron（JST 07:00）/ 手動 | インスタンスを起こしてから `POST /snapshots/run` |

CI では `cargo sqlx prepare --check` により、`.sqlx` のオフラインクエリキャッシュが実際のスキーマと乖離していないかを検証しています。これがないと、マイグレーションを変更したのにキャッシュを再生成し忘れたまま Docker ビルド（`SQLX_OFFLINE=true`）が通ってしまいます。

日次バッチは常駐スケジューラを持たず、GitHub Actions の `schedule` から HTTP で起動します。バッチ専用トークンでユーザー JWT とは分離しています。

---

## 実装状況

全16タスクのロードマップを完了しました。

| # | タスク | 状態 |
|---|---|---|
| 1 | プロジェクト雛形 / `/health` / Docker Compose | 完了 |
| 2 | マイグレーション 0001（users / accounts） | 完了 |
| 3 | AppError 整備 | 完了 |
| 4 | 認証（register / login / JWT） | 完了 |
| 5 | 口座 CRUD | 完了 |
| 6 | 銘柄・価格 | 完了 |
| 7 | `domain::position`（総平均法・評価損益） | 完了 |
| 8 | 取引 CRUD | 完了 |
| 9 | `/holdings` エンドポイント | 完了 |
| 10 | FxRateProvider（Frankfurter） | 完了 |
| 11 | 資産推移（`GET /analytics/asset-history`） | 完了 |
| 12 | 資産配分（`GET /analytics/allocation`） | 完了 |
| 13 | CSV インポート（取引履歴の一括取込） | 完了 |
| 14 | 日次スナップショット（バッチ） | 完了 |
| 15 | OpenAPI（utoipa / Swagger UI） | 完了 |
| 16 | デプロイ・GitHub Actions | 完了 |

### 次の予定

| 優先 | 項目 | 内容 |
|---|---|---|
| 1 | XIRR | 金額加重収益率。入金タイミングを考慮した実質的なパフォーマンス |
| 2 | Google ログイン（OIDC） | 現行の register / login + JWT の上に追加 |

---

## ライセンス

MIT