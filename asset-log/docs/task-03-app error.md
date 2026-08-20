# タスク#3 作業メモ: AppError 整備（制約違反のHTTPマッピング）

- **対象**: 資産管理API（asset-log）
- **完了日**: 2026-08-19
- **成果物**: `src/error.rs`, `src/main.rs`（import修正）, `Cargo.toml`

---

## 1. ゴールと完了条件

| # | 完了条件 | 結果 |
|---|---|---|
| 1 | Postgres の `23505` / `23514` / `23503` が 409 / 422 / 422 にマッピングされる | OK |
| 2 | `sqlx::Error::RowNotFound` が 404 になる | OK |
| 3 | 5xx でクライアントに内部情報が漏れない（ログには全文が残る） | コード上は担保。実発生での検証はタスク#5に持ち越し |
| 4 | レスポンス本文が RFC 9457（`application/problem+json`）準拠 | OK |
| 5 | ハンドラが `?` だけで書ける（`map_err` の連鎖が不要） | OK |

条件3を「未検証」のまま完了としたのは、DB停止のような異常系を意図的に起こす仕組みが現時点で無いため。タスク#5で口座CRUDのテストを書く際に、DBコンテナを止めた状態で500が返ることを確認する。

---

## 2. 実装した内容

### 2.1 AppError のバリアント

| バリアント | ステータス | 用途 |
|---|---|---|
| `BadRequest(String)` | 400 | リクエスト形式の不正 |
| `Unauthorized` | 401 | 認証なし・トークン不正（タスク#4で使用） |
| `NotFound(&'static str)` | 404 | リソース不在 |
| `Conflict(String)` | 409 | UNIQUE 制約違反 |
| `UnprocessableEntity { detail, errors }` | 422 | CHECK 制約違反・バリデーション |
| `Database(sqlx::Error)` | 500 | 分類できなかった DB エラー |
| `Internal(anyhow::Error)` | 500 | それ以外 |

タスク#1時点では `NotFound` / `Database` / `Internal` の3つのみだった。

### 2.2 SQLSTATE のマッピング

```
23505 UNIQUE_VIOLATION      -> 409 Conflict
23514 CHECK_VIOLATION       -> 422 Unprocessable Entity
23503 FOREIGN_KEY_VIOLATION -> 422
23502 NOT_NULL_VIOLATION    -> 422
sqlx::Error::RowNotFound    -> 404 Not Found
上記以外                     -> 500（Database バリアントに保持）
```

### 2.3 制約名 → メッセージの対応表

マイグレーション0001で作成した制約の実名に合わせて定義。

| 制約名 | 種別 | メッセージ |
|---|---|---|
| `users_email_lower_key` | UNIQUE | このメールアドレスは既に登録されています |
| `accounts_user_name_key` | UNIQUE | 同じ名前の口座が既に存在します |
| `accounts_currency_format` | CHECK | 通貨コードは ISO 4217 の大文字3文字で指定してください |
| `accounts_name_not_blank` | CHECK | 口座名を空にはできません |
| `accounts_withholding_only_tokutei` | CHECK | 源泉徴収区分は特定口座のみ指定できます（特定口座では必須です） |

### 2.4 レスポンス形式（RFC 9457）

```json
{
  "type": "/errors/conflict",
  "title": "Conflict",
  "status": 409,
  "detail": "このメールアドレスは既に登録されています",
  "trace_id": "c730de1a-e381-461e-824a-254f7b156605"
}
```

`Content-Type: application/problem+json`。`errors` フィールドは空のとき省略される。

---

## 3. 設計判断とその根拠

### 3.1 分類は `IntoResponse` ではなく `From<sqlx::Error>` で行う

`Database` バリアントに一旦丸めて、レスポンス化のタイミングで SQLSTATE を見て分岐する案もあった。採らなかった理由は、それだと「このエラーは409になる」という情報が型に現れず、ハンドラを読んでも挙動が分からないため。

`From` の時点で `Conflict` / `UnprocessableEntity` に変換しておけば、ハンドラで `?` を書いた瞬間に正しいステータスが確定する。結果としてハンドラ側から分岐が完全に消えた。

### 3.2 制約名の対応表を持ち、フォールバックを用意する

汎用の `From` は「どのカラムの重複か」を知らない。しかし Postgres は違反した制約名を返すので、表を1つ持てばハンドラ側に分岐を書かずに文言を出し分けられる。

重要なのは、表に無い制約は汎用文言にフォールバックさせている点。マイグレーションを追加して表への追従を忘れても、**ステータスコードは壊れない**。文言が汎用になるだけで済む。

### 3.3 個別対応は `OnConstraint` トレイトに逃がす

タスク#4の register のように、特定の制約だけ文言やバリアント（`Conflict` ではなく `field` エラー）を変えたいケースがある。そのためだけに対応表を複雑にするのではなく、拡張トレイトを1つ用意した。

```rust
.on_constraint("users_email_lower_key", || {
    AppError::field("email", "このメールアドレスは登録済みです")
})?
```

### 3.4 5xx は `trace_id` のみ返す

`sqlx::Error` の Display には SQL 断片やカラム名が含まれることがある。そのままクライアントに返すとスキーマ情報が漏れるため、固定文言 + UUID のみ返し、詳細はログ側に `error = ?self`（Debug、`#[source]` 込み）で残す。ログとレスポンスは `trace_id` で突合する。

### 3.5 `type` は安定した相対参照にする

`/errors/conflict` のような固定文字列にした。クライアントはステータスコードだけでなくこの値で分岐できる。将来ドキュメントを公開する際は絶対URIに変更するが、その時もパス部分は変えない。

### 3.6 FOREIGN KEY 違反を 422 とした（暫定）

`POST /accounts` で存在しない `user_id` を指定した場合、404 で返す設計もあり得る。ただし JWT から `user_id` を取る前提だと、FK違反は「トークンは有効だがユーザーが削除済み」という異常系になり、422 より 401 が自然。タスク#4で認証を入れた時点で再検討する。

---

## 4. つまずいた点と教訓

### 4.1 説明用の擬似コードをファイルに貼り込んでしまった

**症状**: `src/domain/account_type.rs` で `error: expected item, found '.'`。

**原因**: 「使い方のイメージ」として示された `sqlx::query_as!(Account, "INSERT INTO accounts (...) VALUES (...)")` を、実コードとして貼り付けていた。`(...)` や `...` はプレースホルダであり、そもそもコンパイルできない。

**教訓**: `...` や `(...)` を含むコードは説明用。貼る前に、省略記号が残っていないかを確認する。

### 4.2 `.route()` をファイルのトップレベルに置いた

**症状**: `src/main.rs:98:1 error: expected item, found '.'`。

**原因**: `.route(...)` は `Router::new()` から続くメソッドチェーンの一部であり、単独の文にはならない。関数の外に置いたため、Rust が「モジュール直下に来るべき項目」を期待している位置でピリオドを見つけた。

**教訓**: `.` で始まる行は必ず直前の式に接続している。追加位置は「どの式の途中か」で決まる。特に axum のルーターでは `.with_state()` より**前**に `.route()` を置く必要がある（`with_state` を呼ぶと状態型が `()` に確定するため）。

### 4.3 `use` の重複

**症状**: `error[E0252]: the name 'get' is defined multiple times`。

**原因**: 既存の `use axum::{routing::get, Router};` がある状態で、`use axum::{extract::State, http::StatusCode, routing::get, Router};` を別行として追加した。

**教訓**: import の指示を受けたら「追加」か「置換」かを既存行と照合してから適用する。同じクレートからの import は1行にまとめる。

### 4.4 `skip_serializing_if` の関数がスライスに対応していなかった

**症状**:
```
error[E0308]: mismatched types
  --> src/error.rs:187:10
   | expected `&Vec<_, _>`, found `&&[FieldError]`
```

**原因**: フィールドの型を `&'a [FieldError]`（スライス）にしたのに、`skip_serializing_if = "Vec::is_empty"` を指定していた。`Vec::is_empty` は `&Vec<T>` を受け取る関数なので型が合わない。

**対処**: `skip_serializing_if = "<[FieldError]>::is_empty"` に変更。スライスのメソッドなら `&[FieldError]` を受け取れる。

**教訓**: `skip_serializing_if` は「そのフィールドの型への参照」を引数に取る関数パスを書く。フィールドの型を変えたら、ここも合わせて見直す。

### 4.5 ホストから起動する際の `DATABASE_URL` 未設定

**症状**: `panicked at src/config.rs:10: DATABASE_URL must be set: NotPresent`。

**原因**: `.env` の `DATABASE_URL` はコンテナ用に `db:5432` を指しており、Compose が読むだけでホストのシェルには入らない。またターミナルを開き直すと `export` した変数は消える。

**教訓**: ホストから `cargo run` する場合は毎回 `export DATABASE_URL='postgres://...@localhost:5432/...'` が必要。タスク#2で `sqlx` CLI に `SQLX_DB_URL` を使った運用と同じ問題。`direnv` の導入を検討する余地がある。

### 4.6 404 の正体がコンテナの古いバイナリだった

**症状**: 実装したはずの `/_debug/conflict` が `404 Not Found`、`content-length: 0`。

**原因**: ホストで編集しただけで、8080 を握っていたのは Docker の `api` コンテナ（タスク#1時点のバイナリ）。ホストから `cargo run` するには先に `docker compose stop api` が必要だった。

**教訓**: `content-length: 0` の 404 は axum のデフォルトフォールバック。自前の `AppError` を通っていれば `application/problem+json` が付くので、**Content-Type で「どのプロセスが返したか」を判別できる**。ポートが誰に握られているかを最初に疑う。

### 4.7 編集後に再ビルドせず、古いバイナリで検証していた

**症状**: 制約名の対応表を修正したのに、`detail` がフォールバック文言（「既に登録されている値です」）のまま変わらない。

**原因**: `cargo run` はコンパイルするが、**起動中のプロセスには反映されない**。`Ctrl+C` で止めて再実行する必要があった。当初は sqlx が制約名を拾えていない可能性を疑ったが、`tracing::warn!` でログを入れたところ `constraint=Some("users_email_lower_key")` と正しく取れており、単に古いプロセスが応答していただけと判明した。

**教訓**: 「直したのに変わらない」ときは、まず動いているバイナリが新しいかを確認する。ターミナルAのログに `Compiling` が出たかが判断材料になる。原因の仮説を立てる前に、この確認を先に済ませる。

### 4.8 `ON CONFLICT DO NOTHING RETURNING` は2回目に行を返さない

**症状**: 検証用の `/_debug/check` が2回目の実行で 404。

**原因**: `ON CONFLICT DO NOTHING RETURNING id` は競合時に行を返さないため、`fetch_one` が `RowNotFound` になる。

**教訓**: 意図した挙動ではあり、結果的に `RowNotFound → 404` のマッピングが動いている証拠にもなった。ただし検証用ルートは繰り返し実行する前提なので、冪等になるよう `INSERT ... ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id` にするか、事前に DELETE する運用にする。

---

## 5. 検証方法

一時的なデバッグルートを `main.rs` に追加して確認し、確認後に削除した。

| ルート | 操作 | 期待 | 結果 |
|---|---|---|---|
| `/_debug/conflict` | 同一メールを大小文字違いで2回 INSERT | 409 + メール重複文言 | OK |
| `/_debug/check` | iDeCo 口座に `withholding=false` を INSERT | 422 + 源泉徴収文言 | OK |
| `/_debug/check`（2回目） | 同上 | 404 | OK |

ログ出力（検証時のみ `tracing::warn!` を追加、確認後削除）:

```
WARN asset_log::error: unique violation constraint=Some("users_email_lower_key")
WARN asset_log::error: request rejected trace_id=c730de1a-... status=409 error=このメールアドレスは既に登録されています
WARN asset_log::error: check violation constraint=Some("accounts_withholding_only_tokutei")
WARN asset_log::error: request rejected trace_id=6cf46926-... status=422 error=源泉徴収区分は特定口座のみ指定できます（特定口座では必須です）
```

---

## 6. 次タスクへの引き継ぎ

### タスク#4（認証: register / login / JWT）で使うもの

- **メール重複**: `on_constraint("users_email_lower_key", || AppError::field("email", "..."))` で 422 のフィールドエラーとして返すか、対応表任せで 409 にするかを決める。REST の慣習としては register の重複は 409 が一般的
- **`Unauthorized` / `Forbidden`**: 現在 dead_code 警告が出ているバリアント。JWT 検証ミドルウェアで初めて使われる
- **FOREIGN KEY の扱い**: 3.6 の通り、認証導入時に 422 のままで良いか再検討する

### 未消化の項目

- 完了条件3（5xx で内部情報を漏らさない）の実発生での検証
- `Validation` を `validator` クレートと接続する処理。現在 `AppError::field()` は手書き前提

### 警告について

`cargo build` 時に以下の dead_code 警告が出るが、いずれもタスク#4以降で使われる。

```
variants `BadRequest`, `Unauthorized`, and `Forbidden` are never constructed
associated function `field` is never used
trait `OnConstraint` is never used
type alias `AppResult` is never used
```

---

## 7. 実行コマンド一覧（再現用）

```bash
cd ~/workspace/shisan-api

# ホストから起動する場合は api コンテナを止める（db は残す）
docker compose stop api

cd asset-log
export DATABASE_URL='postgres://assetlog:***@localhost:5432/assetlog'
cargo run

# 別ターミナルで
curl -i localhost:8080/health
curl -i localhost:8080/_debug/conflict
curl -i localhost:8080/_debug/check

# 制約名の確認
docker compose exec db psql -U assetlog -d assetlog -c "\d users"
docker compose exec db psql -U assetlog -d assetlog -c "\d accounts"

# テストデータ削除
docker compose exec db psql -U assetlog -d assetlog \
  -c "DELETE FROM users WHERE email IN ('dup@example.com','check@example.com')"

# コンテナ側に戻す
cd ~/workspace/shisan-api
docker compose up --build -d
```