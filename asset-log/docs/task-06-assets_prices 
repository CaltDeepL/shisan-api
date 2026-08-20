# タスク#6 銘柄CRUD + 価格登録

## 1. ゴールと完了条件

銘柄マスタ（`assets`）と価格履歴（`asset_prices`）を実装し、取引登録（タスク#8）と
評価損益計算（タスク#7・#9）が乗る土台を作る。

ロードマップ上、このタスクだけ完了条件が未設定だったため、着手時に以下を定めた。

> **完了条件**: `tests/assets_test.rs` が green（8項目）

実際には401の確認を独立させたため9本になった。

| テスト | 検証内容 |
|---|---|
| `create_and_search_asset` | 登録→一覧→単体取得、`?q=` の部分一致、`user_id` が漏れないこと |
| `duplicate_symbol_conflicts` | 大文字小文字違いでも symbol 重複 → 409 |
| `other_users_asset_is_not_found` | GET・PATCH とも404、一覧にも出ない。他人が同じ symbol を登録できる |
| `invalid_input_is_rejected` | `price_unit: 0` / 不正な通貨コード / 空白のみの symbol・name → 422、空PATCH → 400 |
| `price_unit_defaults_by_asset_class` | 省略時に投信は10000、株式は1。明示指定が優先される |
| `price_upsert_overwrites_same_day` | 同一日の再登録が上書き。同一リクエスト内の日付重複も後勝ちで通る |
| `price_batch_is_all_or_nothing` | 1件でも不正なら全件未登録。空配列400、未来日422 |
| `price_requires_owned_asset` | 他人の・存在しない `asset_id` への登録・履歴取得とも404 |
| `requires_authentication` | トークン無しで401 |

結果: `9 passed; 0 failed`（2.17s）

---

## 2. 構築したもの

### マイグレーション

| ファイル | 内容 |
|---|---|
| `0002_assets.sql` | `asset_class` ENUM、`assets` テーブル |
| `0003_prices.sql` | `asset_prices` テーブル |

`assets` の制約:

- `UNIQUE INDEX assets_user_symbol_key ON assets (user_id, upper(symbol))`
- `CHECK`: `assets_symbol_not_blank` / `assets_name_not_blank` /
  `assets_currency_format` / `assets_price_unit_positive`
- `user_id` → `users(id)` は `ON DELETE CASCADE`
- `assets_set_updated_at` トリガ（`accounts` と同じ関数を再利用）

`asset_prices` の制約:

- 主キーは `(asset_id, priced_on)` の複合
- `asset_id` → `assets(id)` は `ON DELETE CASCADE`
- `CHECK`: `asset_prices_price_non_negative` / `asset_prices_source_not_blank`

### コード

```
src/
  domain/asset.rs           AssetClass, Asset, AssetPrice
  repository/asset_repo.rs  create / list / find / update / escape_like
  repository/price_repo.rs  upsert_many / history
  handler/assets.rs         DTO + 4ハンドラ
  handler/prices.rs         DTO + 2ハンドラ
tests/assets_test.rs        統合テスト9本
```

### エンドポイント

| Method | Path | 備考 |
|---|---|---|
| GET | `/assets` | `?q=` で symbol・name の部分一致検索 |
| POST | `/assets` | 201 |
| GET | `/assets/{id}` | |
| PATCH | `/assets/{id}` | `symbol` / `name` / `price_unit` のみ |
| POST | `/prices` | 単体・一括とも同じ形。UPSERT |
| GET | `/prices/{asset_id}` | `?from=` `?to=` |

---

## 3. 設計判断

| 論点 | 決定 | 理由 |
|---|---|---|
| 銘柄の所有 | `user_id NOT NULL`（全ユーザー所有） | 認可がタスク#5と同じ「他人のものは404」で統一できる。共通マスタ併用は `COALESCE(user_id, ゼロUUID)` の部分UNIQUEと更新権限の分岐が要る。後から `DROP NOT NULL` で共通マスタ化する方向は容易で、逆は難しい |
| symbol の一意性 | `(user_id, upper(symbol))` の関数インデックス | `voo` と `VOO` を別銘柄にしない。`users_email_lower_key` と同じ方式 |
| `asset_class` の値 | 小文字 snake_case | 実装済みの `account_type` に揃える |
| `price_unit` | `assets` に保持、既定値は区分から導出 | 投信の基準価額は1万口あたり。1のまま登録されると評価額が1万倍になる |
| PATCH 対象 | `symbol` / `name` / `price_unit` のみ | `currency` と `asset_class` を変えると、既存の取引の解釈（円換算するか）が遡って変わり、過去の損益が黙って書き換わる |
| `Option<Option<T>>` | 使わない | `assets` は全列 NOT NULL のため、「未指定」と「明示的な null」を区別する必要がない。三値表現は NULL 許容列にだけ使う |
| DELETE `/assets` | 今回は実装しない | 取引が紐づく銘柄の扱いは `transactions.asset_id` のFK方針（タスク#8）が決まってから |
| 同日価格の再登録 | UPSERT | 価格は訂正が入るデータで、日次バッチ（タスク#13）も同じ日を何度も書く。409にすると呼ぶ側が必ず先読みを強いられる |
| 単体登録 | 配列1件で受ける | 専用の形を別に持つとDTOもテストも2倍になり、片方だけ検証が抜ける |
| 価格の認可 | `assets` との JOIN に埋め込む | 存在確認を別クエリにすると、確認と挿入の間に銘柄が消える隙間が生まれる。`asset_prices` に `user_id` を持たせると二重管理になる |
| レスポンスの数値 | 文字列（`rust_decimal::serde::str`） | JavaScript 側の精度落ちを防ぐ |
| レスポンス型 | `Asset` とは別の `AssetResponse` | `user_id` の漏洩をコンパイル時に防ぐ |

### 認可を SQL に埋め込む形

```sql
INSERT INTO asset_prices (asset_id, priced_on, price, source)
SELECT a.id, u.priced_on, u.price, $5
FROM assets a
CROSS JOIN UNNEST($3::date[], $4::numeric[]) AS u(priced_on, price)
WHERE a.id = $1 AND a.user_id = $2
ON CONFLICT (asset_id, priced_on)
DO UPDATE SET price = EXCLUDED.price, source = EXCLUDED.source
```

所有していなければ結合結果が0行になり、1行もINSERTされない。
`rows_affected() == 0` を handler が404に変換する。
そのため**空配列は handler で先に400にする必要がある**。
順序が逆だと「空リクエスト」が「銘柄が無い」に化ける。

---

## 4. つまずいた点と教訓

### 4.1 `SQLX_DB_URL` が空で `relative URL without a base`

**症状**: `sqlx migrate run --database-url "$SQLX_DB_URL"` が
`error with configuration: relative URL without a base`。

**原因**: `export` はシェルセッション限りで、ターミナルを開き直した時点で消えていた。
sqlx CLI は空文字列を「相対URL」として解釈する。

**教訓**: DB接続情報は `asset-log/.env` に `DATABASE_URL` として置く。
sqlx CLI はカレントディレクトリの `.env` を自動で読むため
`--database-url` の指定自体が不要になる。
ただし**この `.env` を `source` しない**こと。した瞬間にシェル環境変数になり、
Compose がそちらを優先して api コンテナの接続先が `localhost` に化ける
（タスク#1の教訓と同じ罠）。

### 4.2 `.env` の記述ミス3種

いずれもエラーにならず静かに間違う。

| 症状 | 原因 |
|---|---|
| 認証エラー | `DATABASE_URL` のパスワードがドキュメント上のマスク `***` のままだった |
| `JWT_SECRET` が意図した値にならない | 改行が抜けて `PGADMIN_DEFAULT_PASSWORD=...JWT_SECRET='...'` と1行に潰れ、2つ目の変数が存在していなかった |
| SQLログが出ない | `RUST_LOG` が二重定義され、後勝ちで `info` になっていた |

**教訓**: `.env` は差分が見えないため、値の取り違えに気付きにくい。
`POSTGRES_*` と `PGADMIN_*`（Compose 用）はリポジトリルート、
`DATABASE_URL` / `PORT` / `RUST_LOG` / `JWT_*`（クレートと sqlx CLI 用）は
`asset-log/` と、**置き場所で役割を分ける**。同じ変数を2箇所に書かない。

### 4.3 `psql` はホストに存在しない

**症状**: `psql "$SQLX_DB_URL"` が `/tmp/.s.PGSQL.5432` へのソケット接続で失敗。
`\d assets` は `zsh: command not found: d`。

**原因**: Postgres はコンテナ内にしかない。引数が空文字列だと psql は
ローカルの Unix ソケットに繋ぎに行く。`\d` は psql のメタコマンドで、
シェルからは呼べない。

**教訓**: `docker compose exec db psql -U assetlog -d assetlog` で
コンテナ内の psql に入る。`-c '\d assets'` で単発実行もできる。

### 4.4 関数の中身をモジュール直下に貼った

**症状**: `error: expected item, found keyword 'let'` /
`'let' cannot be used for global variables`。

**原因**: 説明用に切り出したコード片（`let mut dedup = ...`）を
`price_repo.rs` の直下に貼った。Rustのモジュール直下に置けるのは
`fn` / `struct` / `use` / `const` などの item だけ。

**教訓**: このエラーは常に「貼り付け位置が関数の外」を意味する。
構文エラーではなく配置の問題として読む。

### 4.5 既存コードの規約を確認せずに書いて57エラー

**原因**: 以下を実物を見ずに書いた。

| 誤り | 実際 |
|---|---|
| `state.pool` | `state.db` |
| `user.user_id` | `user.0`（`AuthUser` はタプル構造体） |
| `AppError::NotFound` | `AppError::NotFound(&'static str)` |
| `AppError::bad_request(...)` | `AppError::BadRequest(String)` |

加えて `handler/prices.rs` は `use` ブロックごと書き漏らしていた。

**教訓**: 新しいファイルを追加する前に、同じ層の既存ファイル
（今回なら `handler/accounts.rs`）を開いて、
`AppState` のフィールド名・認証抽出子の形・`AppError` のコンストラクタを
先に確認する。1ファイル読むコストの方が、57件のエラーを追うより安い。

### 4.6 `expected Result<...>, found ()` は関数の末尾を見る

**症状**: `patch_asset` で型エラー。エラー箇所が関数の閉じ括弧全体を指していた。

**原因**: 検証ブロックを**追記**した結果、末尾の `asset_repo::update(...)` が消え、
`let patch = ...` が2箇所に重複していた。

**教訓**: このエラーは「戻り値の式が無いまま関数が終わっている」サイン。
個々の行ではなく関数の最後を見る。
関数の途中に処理を足すときは、追記ではなく**関数まるごと差し替える**方が安全。

### 4.7 `.with_state(state)` の後ろにルートを足した

**症状**: `expected Router, found Router<AppState>`。

**原因**: `with_state` は `Router<AppState>` を `Router<()>` に変換する終端処理。
その後ろに `State<AppState>` を要求するハンドラを足すと型が合わない。

**教訓**: `with_state` は常にチェーンの最後。新しいルートはその前に足す。

### 4.8 `lib.rs` の記法が混在した

**症状**: `the name 'auth' is defined multiple times`、`get` / `post` の二重 import。

**原因**: 既存の `lib.rs` は `handler::accounts::list` とフルパスで書くスタイルだったのに、
`use crate::handler::{accounts, assets, ...}` を追加したため、
`pub mod auth;` と名前が衝突した。

**教訓**: ファイル内の既存の記法に合わせる。混ぜると衝突する。

### 4.9 `ON CONFLICT DO UPDATE` は同じ行を二度更新できない

同一リクエスト内に同じ `priced_on` が2件あると、
SQLSTATE `21000`（cardinality_violation）で落ちる。
`upsert_many` の中で `BTreeMap` に入れて後勝ちに正規化することで回避した。

呼び出し側の責務にすると、忘れた経路だけが本番で落ちる。
**リポジトリ層の中で正規化する**方が安全。

### 4.10 `CHECK` に `current_date` は書けない

`CHECK (priced_on <= current_date)` は `current_date` が IMMUTABLE でないため
PostgreSQL に拒否される（ダンプ・リストア時に過去のデータが制約違反になるため）。
未来日の検証は handler 層に置いた。

**DB側で守れない唯一の制約**なので、タスク#8の `traded_on` でも同じ判断が必要。

---

## 5. 次タスクへの引き継ぎ

### タスク#7（`domain::position`）で使うもの

- `AssetClass::is_priceable()` — `cash` のみ `false`。常に額面評価するため価格を引かない
- `Asset::price_unit` — 評価額は `保有数量 × 現在価格 ÷ price_unit × 為替レート`

`price_unit` が `0` にならないことは `assets_price_unit_positive` で保証済みなので、
`evaluate()` 側でゼロ除算を気にする必要はない。ただし**保有数量0**のケースは
domain 側でテストすること（必須8ケースの7番）。

### タスク#8（取引CRUD）で必要な対応

- `transactions.asset_id` → `assets(id)` は **`ON DELETE RESTRICT`**。
  `asset_prices` を CASCADE にしたのと**意図的に非対称**にする。
  価格履歴は銘柄と運命を共にしてよいが、取引履歴が消えるのは事故
- `DELETE /assets` はこのFK方針が決まってから追加する。
  取引が紐づく銘柄の削除は FK 違反（23503）→ 422 になる
- `traded_on` の未来日チェックは 4.10 と同じ理由で handler に置く

### タスク#9（`/holdings`）で必要になるクエリ

「各銘柄の最新価格」を引く関数がまだ無い。`DISTINCT ON` で書く想定。

```sql
SELECT DISTINCT ON (asset_id) asset_id, priced_on, price
FROM asset_prices
WHERE asset_id = ANY($1)
ORDER BY asset_id, priced_on DESC
```

### 運用上の注意

- `cargo sqlx prepare -- --all-targets` の実行忘れは引き続き最大の落とし穴。
  `prepare` は内部で `cargo check` を走らせるため、**コンパイルが通らない限り成功しない**。
  順番は常に「`cargo check` を通す → `prepare`」
- `escape_like` は `?q=` の `%` `_` を無効化するためのもの。
  検索条件を足すときは通し忘れない

---

## 6. 残課題

- **制約名マッピングの実効性が未検証**。統合テストの422はすべて handler の
  バリデーションで先に弾かれており、DBの `CHECK` 違反が422に変換される経路を
  通っていない。リポジトリ層を直接叩くテストを1本足すか、タスク#8で確認する
- **テストの日付が固定値**（`2026-08-20` など）。将来この日付を跨ぐと
  未来日チェックに引っかかって失敗する。`chrono::Utc::now()` からの相対に直す
- `GET /prices/{asset_id}` にページングが無い。日次価格が数年分溜まると
  レスポンスが膨らむ。タスク#11の時系列APIを作るときに合わせて検討する

---

## 7. 実行コマンド一覧（再現用）

```bash
cd ~/workspace/shisan-api/asset-log

# マイグレーション
sqlx migrate add -r 0002_assets
sqlx migrate add -r 0003_prices
# ... SQL を書いてから
sqlx migrate run
sqlx migrate info

# スキーマ確認
cd ~/workspace/shisan-api
docker compose exec db psql -U assetlog -d assetlog -c '\d assets'
docker compose exec db psql -U assetlog -d assetlog -c '\d asset_prices'
docker compose exec db psql -U assetlog -d assetlog -c '\dT+ asset_class'

# 往復確認
cd asset-log
sqlx migrate revert   # 0003
sqlx migrate revert   # 0002
sqlx migrate run

# ビルドとオフラインデータ
cargo check
cargo sqlx prepare -- --all-targets

# テスト
cargo test --test assets_test
```