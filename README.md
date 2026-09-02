# shisan-api

[![CI](https://github.com/CaltDeepL/shisan-api/actions/workflows/ci.yml/badge.svg)](https://github.com/CaltDeepL/shisan-api/actions/workflows/ci.yml)

**NISA・iDeCo・特定口座・一般口座を横断して、資産・損益・収益率を管理する資産管理 API。**

証券会社ごとにアプリを開いて残高を確認する手間をなくし、「制度をまたいだ資産全体で、実際にいくら増えたのか」を一箇所で把握することを目的としています。

**デモ**: https://shisan-api.onrender.com/docs

<!-- TODO: Swagger UI のスクリーンショットを配置
![Swagger UI](docs/images/swagger-overview.png)
-->

Swagger UI からブラウザ上で全エンドポイントを試せます。`POST /auth/register` でアカウントを作成し、返却されたトークンを右上の **Authorize** に入力してください。

> 無料プランで稼働しているため、アクセスがない間はインスタンスが停止します。最初のリクエストは応答まで数十秒かかることがあります。

> **ポートフォリオプロジェクトです。** バックエンド全16タスクのロードマップを完了し、CI・自動デプロイ・日次バッチが稼働しています。現在はフロントエンド（React SPA）を実装中です。

---

## なぜ作ったか

既存の家計簿アプリでも残高の合算はできますが、以下が扱いにくいと感じました。

**NISA の非課税枠が制度どおりに管理されない。** 2024年以降の新NISAは「つみたて投資枠」と「成長投資枠」で年間上限が異なるため、枠を区別せずに集計すると意味をなしません。

| 枠 | 年間上限 |
|---|---|
| つみたて投資枠 | 120万円 |
| 成長投資枠 | 240万円 |
| 年間合計 | 360万円 |
| 生涯投資枠 | 1,800万円 |

制度仕様: [金融庁 NISA特設ウェブサイト](https://www.fsa.go.jp/policy/nisa2/index.html)

**収益率が単純な損益率でしか出ない。** 積立のように入金タイミングがばらつく場合、単純な損益率では実質的なパフォーマンスがわかりません。金額加重収益率（XIRR）が必要です。

**税引後のリターンが見えない。** 特定口座と非課税口座が混在していると、額面の損益と手元に残る額が乖離します。

これらを扱うには、口座種別と取引履歴を制度に即した形でモデリングする必要があります。**その設計自体がこのプロジェクトの主題です。**

---

## Features

| 機能 | エンドポイント |
|---|---|
| 認証（JWT） | `POST /auth/register` `POST /auth/login` |
| 口座管理（6種の口座種別） | `/accounts` |
| 銘柄・価格 | `/assets` `/assets/{id}/prices` |
| 取引履歴（総平均法で取得単価を算出） | `/transactions` |
| 保有一覧・評価損益 | `GET /holdings` |
| 資産推移 | `GET /analytics/asset-history` |
| 資産配分 | `GET /analytics/allocation` |
| CSV インポート（dry-run 対応） | `POST /transactions/import` |
| 為替換算（ECB レート） | 外部 API 連携 |
| 日次スナップショット | `POST /snapshots/run` |
| API 仕様 | `GET /openapi.json` `/docs` |

---

## Architecture

レイヤードアーキテクチャを採用し、計算ロジックを I/O から分離しています。

```
        handler          HTTP の入出力のみ。リクエスト検証とレスポンス整形
           ↓
        service          ユースケースの組み立て
           ↓
       repository        DB アクセス。SQL はここに閉じる
           ↓
       PostgreSQL

        domain           純粋関数。総平均法による取得単価、評価損益
                         （I/O を持たないため単体でテスト可能）

        provider         外部依存を trait で抽象化
           ├── FxRateProvider   （Frankfurter / ECB レート）
           └── PriceProvider
```

### ディレクトリ構成

```
shisan-api/
├── compose.yaml              # Postgres + API
├── .env                      # Compose 用の環境変数（gitignore 対象）
├── .github/workflows/        # CI / Deploy / Daily Snapshot
├── web/                      # フロントエンド（Vite + React + TypeScript）
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
        ├── cors.rs           # CORS レイヤ
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

## Domain Model

### 口座種別（`account_type` ENUM）

| 値 | 意味 | 課税 |
|---|---|---|
| `tokutei` | 特定口座 | 課税 |
| `ippan` | 一般口座 | 課税 |
| `nisa_tsumitate` | NISA つみたて投資枠 | 運用益非課税 |
| `nisa_growth` | NISA 成長投資枠 | 運用益非課税 |
| `ideco` | iDeCo | 掛金控除・運用益非課税・受取時課税 |
| `bank` | 待機資金（現金） | — |

### Position（保有ポジション）

取引履歴から総平均法で取得単価を算出します。同一銘柄でも口座が異なれば別ポジションとして扱います（非課税枠の管理単位が口座だから）。

- 買い増し → 加重平均で取得単価を再計算
- 一部売却 → 取得単価は維持し、実現損益を計上
- 全売却後の再購入 → 取得単価をリセット

---

## Key Design Decisions

設計判断の詳細は [`asset-log/docs/`](asset-log/docs/) にタスクごとのメモとして残しています。以下は主要なものの要約です。

### なぜ ORM ではなく sqlx なのか

コンパイル時に SQL を検証できるため。`query!` マクロが実際のスキーマに対してクエリを検証し、カラム名の誤りや型の不一致をビルド時に検出します。ORM の抽象化を挟むと、生成される SQL が見えにくくなり、資産推移のような集計クエリでパフォーマンスの予測が立ちません。

SQL を書く負担は残りますが、リポジトリ層に閉じ込めることで影響範囲を限定しています。

### なぜ NISA を1種類ではなく2種類の ENUM にしたのか

非課税枠の消費状況を**枠ごとに**集計する必要があるため。つみたて投資枠は年120万円、成長投資枠は年240万円と上限が異なり、合算しても制度上の意味を持ちません。

`nisa` という単一の値にして「枠の種別」を別カラムで持つ設計も考えられますが、その場合「NISA 以外の口座で枠種別が NULL」という状態を別途制約する必要が出ます。ENUM を分けることで、口座種別だけで枠が一意に決まります。

### なぜ `withholding` を ENUM ではなく `Option<bool>` にしたのか

特定口座の源泉徴収区分を `account_type` に含めて `tokutei_withholding` / `tokutei_no_withholding` と分割する案もありましたが、採用しませんでした。源泉徴収は「特定口座の属性」であって口座種別そのものではなく、ENUM に混ぜると口座種別の意味が二重になります。

`accounts.withholding BOOLEAN` を **nullable + CHECK 制約**とし、「特定口座なら必須、それ以外なら NULL」を DB レベルで強制しています。

`NOT NULL DEFAULT false` にしなかった理由が重要です。その場合 iDeCo 口座の `withholding = false` が「源泉徴収なしの特定口座」と区別できなくなります。**「値がない」と「false という値がある」は違う**という区別を、型と制約の両方で表現しています。Rust 側では `Option<bool>` として受け取り、型が意図を語る形にしています。

### なぜ日次スナップショットを正本にしないのか

資産推移の正本はあくまで**取引履歴からの再計算**です。スナップショットはキャッシュに過ぎません。

過去の取引が後から追加・修正されることが実際にあるため（証券会社の CSV を後日まとめて取り込むなど）、スナップショットを正本にすると過去の値が永久に誤ったままになります。取引が変更された場合は影響する日以降のキャッシュを失効させ、次回参照時に再計算します。

レスポンスの `source` フィールドで、キャッシュとフォールバックのどちらを経由したかが分かるようにしています。

なお「未計算」と「保有ゼロ」を区別するため、計算済みマーカーを別テーブル（`snapshot_days`）に分離しています。値が 0 であることと、まだ計算していないことは別の状態です。

### なぜ GitHub Actions から snapshot を起動するのか

アプリケーション内に常駐スケジューラを持たないため。Render の無料プランはアクセスがないとインスタンスが停止するので、プロセス内 cron は動作を保証できません。

GitHub Actions の `schedule` から HTTP で起動する方式なら、インスタンスを起こしてからバッチを叩けます。認証はユーザー JWT とは分離したバッチ専用トークン（`SNAPSHOT_JOB_TOKEN`）を使い、未設定時は 503 で拒否します。

### なぜ Render の auto-deploy を無効にしたのか

Render の auto-deploy は `main` への push を検知して即座にビルドを始めるため、**テストの結果を待ちません**。テストが落ちるコードでもデプロイされてしまいます。

auto-deploy を切り、GitHub Actions の `workflow_run` イベントで CI の完了と結果を受け取り、`main` ブランチかつ成功時に限って Deploy Hook を叩く構成にしました。CI が green のときだけデプロイが走ります。

### なぜ外部 API を trait で抽象化したのか

為替レートの取得（Frankfurter）を `FxRateProvider` trait として定義し、テスト時にモックへ差し替えられるようにしています。

これにより、外部 API の 5xx 応答・タイムアウト時にキャッシュへフォールバックする挙動まで、実際のネットワークなしでテストできます。外部サービスの障害時の振る舞いは本来テストが難しい領域なので、ここは意図的に抽象化の対価を払っています。

### なぜ OpenAPI とルート定義を同じ場所に置くのか

`utoipa-axum` の `OpenApiRouter` を使い、ルート登録とドキュメント生成をまとめています。

```rust
OpenApiRouter::with_openapi(ApiDoc::openapi())
    .routes(routes!(handler::accounts::create, handler::accounts::list))
    .split_for_parts()
```

`routes!()` に渡したハンドラがそのまま axum のルートになり、同時に `#[utoipa::path]` の情報から仕様が組み立てられます。ルートを追加したのにドキュメントを書き忘れる、パスを変更したのに仕様が古いまま、といった乖離が構造的に起きません。

統合テストでもパス数の完全一致を検証しており、エンドポイントを追加すると仕様の更新を促してテストが落ちます。

---

## 技術スタック

| 領域 | 技術 |
|---|---|
| バックエンド | Rust 1.96 / axum 0.8 |
| DB | PostgreSQL 17 |
| DB アクセス | sqlx 0.9（ORM 不使用） |
| フロントエンド | Vite / React / TypeScript / Tailwind CSS v4 |
| コンテナ | Docker（マルチステージ + distroless） |
| 外部 API | Frankfurter（ECB 為替レート） |
| ホスティング | Render（Docker）/ Neon（Postgres） |
| CI / CD | GitHub Actions |

---

## 実装上の工夫

### エラーレスポンス（RFC 9457）

`AppError` を `IntoResponse` に実装し、Problem Details 準拠の JSON を返します。Postgres のエラーコードを HTTP ステータスにマッピングしています。

| コード | 意味 | HTTP |
|---|---|---|
| `23514` | CHECK 制約違反 | 422 |
| `23505` | UNIQUE 制約違反 | 409 |
| `23503` | 外部キー違反 | 404 / 422 |

制約名とメッセージの対応表を持っており、どのフィールドが原因かをクライアントに返します。5xx では内部情報を露出させず、`trace_id` のみを返します。

### distroless でのヘルスチェック

ランタイムイメージに `gcr.io/distroless/cc-debian12:nonroot` を使っているため、`curl` もシェルも存在しません。そこで clap で `healthcheck` サブコマンドを実装し、バイナリ自身が `/health` を叩く方式に統一しました。

```dockerfile
HEALTHCHECK CMD ["./asset-log", "healthcheck"]
```

reqwest の `blocking` フィーチャーは有効化せず、current-thread のランタイムを起こして非同期クライアントを使っています。TLS も `default-features = false` + `rustls-tls` として OpenSSL への依存を持たず、sqlx と同一の rustls / ring を共有しています。

### CORS

フロントエンド（Render Static Site）と API（Web Service）を別オリジンで運用するため、`tower-http` の `CorsLayer` をルータの最外層に適用しています。

最外層に置くことで、401 などのエラーレスポンスにも CORS ヘッダが付きます。ここが抜けると、ブラウザ側では認証エラーがネットワークエラーとして見え、原因究明が困難になります。

許可オリジンは環境変数から読み、スキームの有無と末尾スラッシュを起動時に検証します。

---

## Testing

```bash
cd asset-log
cargo test --all-targets
```

統合テストは `#[sqlx::test(migrations = "./migrations")]` により、**テストごとに独立した一時 DB** を作成してマイグレーションを適用します。テスト間の状態共有がないため、並列実行しても干渉しません。

`tower::ServiceExt::oneshot` でルータへ直接リクエストを投げる方式を採り、HTTP サーバを起動せずにハンドラからリポジトリまでを通しで検証しています。

外部 API（Frankfurter）は `wiremock` でスタブ化し、正常系に加えて 5xx 応答・タイムアウト時のキャッシュフォールバックまでテストしています。

---

## CI / CD

```
PR
 ↓
CI
 ├─ cargo fmt --check
 ├─ cargo clippy -D warnings
 ├─ cargo test（ユニット + 統合）
 └─ cargo sqlx prepare --check

main merge
 ↓
CI green
 ↓
Render Deploy Hook

GitHub Actions cron（JST 07:00）
 ↓
Daily Snapshot
```

| ワークフロー | トリガー | 内容 |
|---|---|---|
| CI | push / pull_request | fmt / clippy / 全テスト / `sqlx prepare --check` |
| Deploy | CI の成功（main のみ） | Render の Deploy Hook を起動 |
| Daily Snapshot | cron / 手動 | インスタンスを起こしてから `POST /snapshots/run` |

`cargo sqlx prepare --check` により、`.sqlx` のオフラインクエリキャッシュが実際のスキーマと乖離していないかを検証しています。これがないと、マイグレーションを変更したのにキャッシュを再生成し忘れたまま Docker ビルド（`SQLX_OFFLINE=true`）が通ってしまいます。

---

## Setup

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

### フロントエンド

```bash
cd web
npm install
cp .env.example .env.local    # VITE_API_BASE_URL を設定
npm run dev                   # http://localhost:5173
```

### マイグレーション

sqlx CLI はホスト側で実行します。接続先は `asset-log/.env` の `DATABASE_URL` から自動的に読まれます。

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

## API Documentation

起動後、Swagger UI から全エンドポイントの仕様を確認し、その場でリクエストを試せます。

http://localhost:8080/docs

OpenAPI 3.1 の仕様は `/openapi.json` で配信しており、[`asset-log/docs/openapi.json`](asset-log/docs/openapi.json) にもコミットしています。

---

## Implementation Status

### バックエンド（完了）

| # | タスク |
|---|---|
| 1 | プロジェクト雛形 / `/health` / Docker Compose |
| 2 | マイグレーション 0001（users / accounts） |
| 3 | AppError 整備 |
| 4 | 認証（register / login / JWT） |
| 5 | 口座 CRUD |
| 6 | 銘柄・価格 |
| 7 | `domain::position`（総平均法・評価損益） |
| 8 | 取引 CRUD |
| 9 | `/holdings` エンドポイント |
| 10 | FxRateProvider（Frankfurter） |
| 11 | 資産推移（`GET /analytics/asset-history`） |
| 12 | 資産配分（`GET /analytics/allocation`） |
| 13 | CSV インポート |
| 14 | 日次スナップショット（バッチ） |
| 15 | OpenAPI（utoipa / Swagger UI） |
| 16 | デプロイ・GitHub Actions |

### フロントエンド（実装中）

| # | タスク | 状態 |
|---|---|---|
| 17 | CORS 設定 + React SPA 雛形 | 完了 |
| 18 | 認証画面 | 完了 |
| 19 | 口座 CRUD 画面 | 完了 |
| 20 | 銘柄・取引の登録画面 | 完了 |
| 21 | 保有一覧・評価損益 | 完了 |
| 22 | 資産推移・資産配分のグラフ | — |
| 23 | CSV インポート画面 | — |
| 24 | Static Site へのデプロイ | — |

---

## Future Work

| 優先 | 項目 | 内容 |
|---|---|---|
| 1 | XIRR | 金額加重収益率。入金タイミングを考慮した実質的なパフォーマンス |
| 2 | Google ログイン（OIDC） | 現行の register / login + JWT の上に追加 |

---

## License

MIT