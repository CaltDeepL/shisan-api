# shisan-api

NISA・iDeCo を含む複数口座の資産を横断的に管理し、損益と収益率を可視化する資産管理 API です。

証券会社ごとにアプリを開いて残高を確認する手間をなくし、「制度をまたいだ資産全体で、実際にいくら増えたのか」を一箇所で把握することを目的としています。

> **開発中のポートフォリオプロジェクトです。** 全16タスクのロードマップに沿って実装を進めており、API 本体の実装は完了しています。残りは CI とデプロイです。進捗は[実装状況](#実装状況)を参照してください。
---

## なぜ作ったか

既存の家計簿アプリでも残高の合算はできますが、以下が扱いにくいと感じました。

- **NISA の非課税枠が制度どおりに管理されない** — 2024年以降の新NISAは「つみたて投資枠」と「成長投資枠」で年間上限が異なるため、枠を区別せずに集計すると意味をなさない
- **収益率が単純な損益率でしか出ない** — 積立のように入金タイミングがばらつく場合、単純な損益率では実質的なパフォーマンスがわからない。金額加重収益率（XIRR）が必要
- **税引後のリターンが見えない** — 特定口座と非課税口座が混在していると、額面の損益と手元に残る額が乖離する
- **収益率が単純な損益率でしか出ない** — 積立のように入金タイミングがばらつく場合、単純な損益率では実質的なパフォーマンスがわからない。金額加重収益率（XIRR）が必要（16タスク完了後に実装予定）

これらを扱うには、口座種別と取引履歴を制度に即した形でモデリングする必要があります。その設計自体がこのプロジェクトの主題です。

---

## 技術スタック

| 領域 | 技術 | 選定理由 |
|---|---|---|
| バックエンド | Rust 1.96 / axum | 金額計算で型安全性を活かしたい。`Option<T>` と ENUM で「ありえない状態」をコンパイル時に排除する |
| DB | PostgreSQL 17 | ENUM 型、関数インデックス、CHECK 制約でドメイン制約を DB 層でも担保する |
| DB アクセス | sqlx | コンパイル時に SQL を検証できる。ORM を使わず SQL を書く方針 |
| フロントエンド | Vite + React + TypeScript | — |
| コンテナ | Docker / Docker Compose | マルチステージビルド + distroless |
| 外部 API | Frankfurter（為替） | 認証不要で ECB のレートを取得できる |

---

## アーキテクチャ

レイヤードアーキテクチャを採用し、計算ロジックを I/O から分離しています。

```
handler    HTTP の入出力のみ。リクエスト検証とレスポンス整形
   ↓
service    ユースケースの組み立て（holdings_service / analytics_service）
   ↓
repository DB アクセス。SQL はここに閉じる
   ↓
domain     純粋関数。総平均法による取得単価、評価損益、XIRR
```

`provider` 層で為替レートと株価の取得を trait として抽象化し、テスト時にモックへ差し替えられるようにしています。

### ディレクトリ構成

```
shisan-api/
├── compose.yaml              # Postgres + API
├── .env                      # 環境変数（gitignore 対象）
├── src/                      # フロントエンド（Vite + React）
└── asset-log/                # バックエンド（Rust）
    ├── Dockerfile
    ├── migrations/           # sqlx マイグレーション
    ├── docs/                 # タスクごとの設計メモ / openapi.json
    └── src/
        ├── error.rs          # AppError → IntoResponse（RFC 9457）
        ├── openapi.rs        # OpenAPI 定義（ApiDoc / セキュリティスキーム）
        ├── main.rs           # clap CLI + axum 起動
        ├── config.rs         # 環境変数
        ├── state.rs          # AppState / PgPool
        ├── error.rs          # AppError → IntoResponse（RFC 9457）
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
cp .env.example .env    # 値を編集
docker compose up --build -d
curl http://localhost:8080/health
```

### API ドキュメント

起動後、Swagger UI から全エンドポイントの仕様を確認し、その場でリクエストを試せます。

http://localhost:8080/docs


OpenAPI 3.1 の仕様は `/openapi.json` で配信しており、[`asset-log/docs/openapi.json`](asset-log/docs/openapi.json) にもコミットしています。

認証が必要なエンドポイントは、`POST /auth/register` で取得したトークンを Swagger UI 右上の **Authorize** に入力してから試せます。

### マイグレーション

sqlx CLI はホスト側で実行するため、接続先を明示します。

```bash
cd asset-log
export SQLX_DB_URL='postgres://<user>:<password>@localhost:5432/<db>'
sqlx migrate run --database-url "$SQLX_DB_URL"
sqlx migrate info --database-url "$SQLX_DB_URL"
```

> **ホスト名の使い分けに注意**
>
> `.env` の `DATABASE_URL` はコンテナ用に `db:5432` を指しています。`db` は Compose ネットワーク内部のサービス名なので、ホストのシェルからは解決できません。CLI 用には別の変数名（`SQLX_DB_URL`）で `localhost:5432` を渡します。
>
> `DATABASE_URL` を直接 export すると、Compose の変数展開がシェルの環境変数を優先するため、`docker compose up` した API コンテナが自分自身の 5432 を見に行って起動に失敗します。

| 実行主体 | ホスト名 |
|---|---|
| ホストのシェル → db コンテナ | `localhost` |
| api コンテナ → db コンテナ | `db` |

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

ランタイムイメージに `gcr.io/distroless/cc-debian12:nonroot` を使っているため、`curl` もシェルも存在しません。そこで clap で `healthcheck` サブコマンドを実装し、バイナリ自身が reqwest で `/health` を叩く方式に統一しました。Dockerfile の `HEALTHCHECK` 命令と compose.yaml の双方が同じコマンドを呼びます。

```dockerfile
HEALTHCHECK CMD ["./asset-log", "healthcheck"]
```

### ビルドキャッシュ

マルチステージビルドで BuildKit のキャッシュマウントを使い、依存クレートのビルド結果を再利用しています。

### エラーレスポンス

`AppError` を `IntoResponse` に実装し、RFC 9457（Problem Details for HTTP APIs）準拠の JSON を返します。Postgres のエラーコードを HTTP ステータスにマッピングする方針です。

| コード | 意味 | HTTP |
|---|---|---|
| `23514` | CHECK 制約違反 | 422 |
| `23505` | UNIQUE 制約違反 | 409 |
| `23503` | 外部キー違反 | 404 / 422 |

---

## 実装状況

全16タスクのロードマップで進行中です。

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
| 16 | デプロイ・GitHub Actions | 未着手 |

XIRR（金額加重収益率）と Google ログイン（OIDC）は、16タスク完了後の追加機能として [`asset-tracker-design.md`](asset-log/docs/asset-tracker-design.md) に追補しています。

---

## ライセンス

MIT
