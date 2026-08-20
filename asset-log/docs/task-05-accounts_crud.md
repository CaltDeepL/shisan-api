# タスク#5: 口座CRUD

## 1. ゴールと完了条件

| # | 完了条件 | 結果 |
|---|---|---|
| 1 | `POST /accounts` が 201 を返し、同一ユーザー内の口座名重複は 409 | ✅ |
| 2 | `GET /accounts` が自分の口座のみを返す | ✅ |
| 3 | `GET /accounts/{id}` が単体取得できる | ✅ |
| 4 | `PATCH /accounts/{id}` が部分更新でき、「未指定」と「null」を区別する | ✅ |
| 5 | `DELETE /accounts/{id}` が 204 を返し、2回目は 404 | ✅ |
| 6 | 他人の口座は 403 ではなく 404（存在自体を漏らさない） | ✅ |
| 7 | CHECK 制約違反が 422 として日本語メッセージ付きで返る | ✅ |
| 8 | 認証なしは全エンドポイントで 401 | ✅ |
| 9 | 統合テストが green | ✅ 7 passed |

---

## 2. 実装内容

### 追加した依存

```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
serde_json = "1"
```

### 追加・変更したファイル

| パス | 内容 |
|---|---|
| `src/lib.rs` | **新設**。モジュール宣言と `app(state) -> Router` |
| `src/main.rs` | モジュール宣言とルータ構築を `lib.rs` に移譲 |
| `src/domain/account.rs` | `Account` / `AccountType` / `NewAccount` / `AccountPatch` |
| `src/domain/mod.rs` | `account` の宣言 |
| `src/repository/account_repo.rs` | `insert` / `list` / `find` / `update` / `delete` |
| `src/repository/mod.rs` | `account_repo` の宣言 |
| `src/handler/accounts.rs` | DTO 3種と5エンドポイント |
| `src/handler/mod.rs` | `accounts` の宣言 |
| `tests/common/mod.rs` | **新設**。`test_app` / `register_user` / `request` |
| `tests/accounts_test.rs` | **新設**。統合テスト7ケース |
| `Cargo.toml` | dev-dependencies 追加 |
| `.env` | `SQLX_OFFLINE=true` を削除、`DATABASE_URL` を `localhost` に統一 |

### エンドポイント

| メソッド | パス | 成功 | 主な失敗 |
|---|---|---|---|
| POST | `/accounts` | 201 | 409（名前重複）/ 422（CHECK違反） |
| GET | `/accounts` | 200 | 401 |
| GET | `/accounts/{id}` | 200 | 404 |
| PATCH | `/accounts/{id}` | 200 | 400（空ボディ）/ 404 / 422 |
| DELETE | `/accounts/{id}` | 204 | 404 |

---

## 3. 設計判断の根拠

### 3.1 リポジトリ層の全関数が `user_id` を必須引数に取る

```rust
pub async fn find(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<Account>, sqlx::Error>
```

`user_id` 無しの `find_by_id` は**1本も作らない**と決めた。便利だからと1本置くと、後から呼び間違えたときに他人のデータが見える。引数にあれば渡し忘れはコンパイルエラーになる。

「WHERE 句を書き忘れない」という規律ではなく、「書き忘れられない型」にするのが目的。

### 3.2 他人の口座は 403 ではなく 404

403 は「存在するがアクセス権が無い」を意味するため、ID を総当たりすれば口座の存在有無が判定できてしまう。404 に統一すれば、そのユーザーにとってそのリソースは存在しない、という一貫した見え方になる。

テストでは GET / PATCH / DELETE の3メソッドすべてで 404 を確認し、さらに侵入者側の一覧が空であることも検証している。`find` だけスコープを効かせて `list` を忘れる、という漏れ方を捕まえるため。

### 3.3 バリデーションを DB の CHECK に一元化

ハンドラ側に事前バリデーションを**書いていない**。

| 入力 | 検知場所 | 結果 |
|---|---|---|
| 口座名が空白のみ | `accounts_name_not_blank` | 422 |
| 通貨コードが不正 | `accounts_currency_format` | 422 |
| 特定口座で `withholding` 未指定 | `accounts_withholding_only_tokutei` | 422 |
| 特定口座以外で `withholding` 指定 | 同上 | 422 |
| 同一ユーザー内の口座名重複 | `accounts_user_name_key` | 409 |

タスク#3で `From<sqlx::Error>` に制約名→日本語メッセージの対応表を作ってあるので、`?` で投げるだけで正しいステータスとメッセージになる。アプリ側で二重にチェックすると、どちらのメッセージが出るかが読めなくなり、片方だけ直して不整合が残る。

DB を唯一の真実にしておけば、SQL を直接叩かれても不正データは入らない。

### 3.4 PATCH の三値を `Option<Option<T>>` + `CASE WHEN` で表現

JSON の PATCH には3つの状態がある。

| リクエスト | 意味 | Rust 表現 |
|---|---|---|
| キー無し | 変更しない | `None` |
| `"institution": null` | NULL にする | `Some(None)` |
| `"institution": "SBI"` | 値を設定 | `Some(Some(_))` |

serde の既定では `null` が `None` に潰れるため、`deserialize_with` でヘルパーを挟む。

```rust
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de>
{
    Option::<T>::deserialize(de).map(Some)
}
```

`Option<Option<T>>` はそのまま SQL に渡せないので、`(変更するか: bool, 値: Option<T>)` の2引数に分解して SQL 側で分岐する。

```sql
SET name        = COALESCE($3, name),
    institution = CASE WHEN $4 THEN $5 ELSE institution END,
    withholding = CASE WHEN $6 THEN $7 ELSE withholding END
```

`name` だけ `COALESCE` で済むのは NOT NULL 列で「NULL にする」がありえないため。

更新項目が0個の場合だけは DB で検知できない（何も更新しない UPDATE は成功する）ので、そこだけハンドラで 400 を返す。

### 3.5 `updated_at` はトリガに任せる

`accounts_set_updated_at`（BEFORE UPDATE）が既にあるため、UPDATE 文に `updated_at = now()` を**書かない**。書くとトリガと二重になり、どちらが効いているのか読めなくなる。

統合テストで PATCH 後に `updated_at` が変化することを検査しているので、トリガを消したり SQL に書き足したりすれば気付ける。

### 3.6 `Account` に `Serialize` を付けず DTO を挟む

`Account` は `user_id` を持つが、これをレスポンスに出す必要は無い。

```rust
#[derive(Debug, Clone)]          // Serialize は付けない
pub struct Account { pub user_id: Uuid, /* ... */ }

impl From<Account> for AccountResponse { /* user_id をここで落とす */ }
```

`Serialize` が無いので、うっかり `Json(account)` と書くとコンパイルエラーになる。漏洩を規律ではなく型で止める。テスト側でも `created.get("user_id").is_none()` を検査している。

### 3.7 `AccountType` に判定メソッドを持たせる

```rust
pub fn is_tax_exempt(self) -> bool {
    match self {
        Self::NisaTsumitate | Self::NisaGrowth | Self::Ideco => true,
        Self::Tokutei | Self::Ippan | Self::Bank => false,
    }
}
```

`matches!` ではなく `match` で全バリアントを列挙しているのは、ENUM に値を追加したときにここでコンパイルエラーを出すため。タスク#7の損益計算で課税判定を使うので、「新しい口座種別を足したが損益側を直し忘れた」を防ぐ。

### 3.8 `src/lib.rs` の新設

`tests/` 配下の統合テストは**バイナリクレートの中身を `use` できない**。完了条件が「統合テスト green」である以上、ライブラリクレートへの切り出しは必須だった。

ルータ構築も `lib.rs` の `app(state)` に移し、`main.rs` は `asset_log::app(state)` を呼ぶだけにした。テストが「テスト用に組み直したルータ」を検証してしまい本番と乖離する、という事態を避けるため。

### 3.9 テストは `oneshot` + `#[sqlx::test]`

```rust
let response = app.clone().oneshot(request).await?;
```

`tower::ServiceExt::oneshot` でルータに直接リクエストを流す。TCP を立てないので速く、ポート衝突もない。`Router` の clone は内部が `Arc` なので安価。

`#[sqlx::test(migrations = "./migrations")]` はテストごとに独立した一時 DB を作りマイグレーションを流す。おかげで全テストで同じメールアドレスを使い回せる（衝突したらテスト分離が壊れているサインになる）。

テストユーザーの作成はリポジトリ直叩きではなく `POST /auth/register` 経由にした。パスワードハッシュを自前で作る必要が無く、認証経路まで含めて本番と同じ道を通る。「他人の口座は見えない」の検証で、トークンが本物でないと意味が薄れる。

---

## 4. つまずいた点と教訓

### 4.1 `src/lib.rs` が `ilb.rs` になっていた

**症状**: `grep 'mod' src/lib.rs` が「No such file」。`cargo check` は `crate::domain` が見つからないと言う。

**原因**: ファイル名のタイプミス（`i`-`l`-`b`）。Rust から見ると未参照のファイルなので、**エラーも警告も出ずに黙って無視される**。

**教訓**: モジュールツリーに載っていないファイルは存在しないのと同じ。`cargo check` の出力に `Checking <crate>` の行が出ているか、意図したファイルが実際にコンパイルされているかを確認する。

### 4.2 `.env` の `SQLX_OFFLINE=true` が効き続けた

**症状**: シェルで `unset SQLX_OFFLINE` しても「`SQLX_OFFLINE=true` but there is no cached data」が消えない。`env | grep -i sqlx` にも出てこない。

**原因**: sqlx の query マクロは**コンパイル時にクレートルートの `.env` を自力で読む**。`dotenvy` のランタイム呼び出しとは独立した動作なので、シェルの環境変数を消しても無関係。

**教訓**: `SQLX_OFFLINE=true` は Docker ビルド専用。Dockerfile の `ENV` で与え、`.env` には置かない。`.env` に置くと「キャッシュを作るためにキャッシュを見に行く」循環になり、`cargo sqlx prepare` 自体が実行できなくなる。

### 4.3 モジュール宣言の漏れと混入

3種類が同時に起きていた。

| 症状 | 原因 |
|---|---|
| `account_repo.rs` が一切コンパイルされない | `repository/mod.rs` に宣言が無い |
| `E0583: file not found for module accounts` | `handler/mod.rs` に宣言があるが実体が未作成 |
| `E0583` | `domain/mod.rs` に `pub mod account_repo;`（repository 側のもの）が混入 |

**教訓**: `.sqlx` のファイル数を進捗の指標に使うと早い。クエリを5本足したのにファイル数が2のままなら、そのファイルは読まれていない。「エラーメッセージが1文字も変わらない」ときも、そもそもコンパイルされているかを疑う。

### 4.4 貼り付け先の間違いが4回

| 貼った内容 | 貼ってしまった先 | 影響 |
|---|---|---|
| accounts の `query_as!` | `repository/user_repository.rs` | 認証が壊れる |
| `Cargo.toml` の `[dev-dependencies]` | `tests/common/mod.rs` の先頭 | テストがコンパイルできない |
| `DATABASE_URL=... cargo check` | `.env` の6行目 | `PORT=8080` と結合し dotenv パースエラー |
| ルータ構築コード | `lib.rs` の先頭 | `main.rs` と重複 |

**教訓**: 参考例としてのコードと、実際にファイルへ入れるコードを混ぜない。特に `.env` は「1行1変数」であってシェルスクリプトではない。切り分け用のワンライナーは、ファイルではなくターミナルに直接打つ。

### 4.5 `tests/` ディレクトリで `cargo check` を実行

**症状**: `cargo check --tests` が 0.5 秒で「Finished」。エラーも `Checking` 行も出ない。

**原因**: `cd tests` したままだったため、クレートルートの外で実行していた。

**教訓**: `Finished` が速すぎるのは「成功」ではなく「何もしていない」サイン。`Checking <crate名>` の行の有無で判断する。

### 4.6 `tests/common/mod.rs` だけではコンパイルされない

**症状**: `mod.rs` を書いて `cargo check --tests` が通るが、型エラーが1つも出ない。

**原因**: `tests/` 直下に `.rs` ファイルが無いと、cargo はテストターゲットを1つも見つけない。`common/` はサブディレクトリなのでそれ自体はターゲットにならない。

**教訓**: ヘルパーの検証は、それを `mod common;` する実テストファイルを作って初めて可能になる。

### 4.7 `jq -r .token` で 401

**症状**: 正しく取得したはずのトークンで全リクエストが 401。

**原因**: login のレスポンスキーは `token` ではなく `access_token`。`jq` は `null` を返し、`Authorization: Bearer null` を送っていた。

**教訓**: 401 が返ったらまずトークンが空でないか確認する（`echo "${TOKEN:0:20}..."`）。サーバは正しく動いていた。

### 4.8 `ttp-body-util`

**症状**: `error: no matching package named 'ttp-body-util' found`

**原因**: 先頭の `h` が落ちたタイプミス。

**教訓**: crates.io の解決エラーはタイプミスを疑う。エラーメッセージにパッケージ名がそのまま出るので、目視で照合すれば済む。

---

## 5. 検証方法

### 手動確認（curl）

```bash
TOKEN=$(curl -s -X POST localhost:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"t@example.com","password":"***"}' | jq -r .access_token)

# 201
curl -s -X POST localhost:8080/accounts \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name":"SBI特定","account_type":"tokutei","withholding":true}' | jq

# 422（特定口座なのに withholding 未指定）
curl -s -X POST localhost:8080/accounts \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name":"NG口座","account_type":"tokutei"}' | jq

# 400（空の PATCH）
curl -s -X PATCH localhost:8080/accounts/$(uuidgen | tr 'A-Z' 'a-z') \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{}' | jq

# 404（存在しない ID）
curl -s localhost:8080/accounts/$(uuidgen | tr 'A-Z' 'a-z') \
  -H "authorization: Bearer $TOKEN" | jq
```

### 統合テスト

```bash
cargo test --test accounts_test
```

| テスト | 検証内容 |
|---|---|
| `create_and_fetch_account` | 作成→一覧→単体取得、`user_id` が漏れないこと |
| `tokutei_requires_withholding` | 特定口座で `withholding` 省略 → 422 |
| `duplicate_name_conflicts` | 同一ユーザー内の口座名重複 → 409 |
| `patch_distinguishes_null_and_absent` | 未指定は据え置き / `null` は NULL 化 / `updated_at` 更新 / 空ボディ 400 |
| `other_users_account_is_not_found` | GET・PATCH・DELETE すべて 404、一覧も空 |
| `delete_then_gone` | 204 → 2回目 404 → GET も 404 |
| `requires_authentication` | トークン無しで 401 |

結果: `7 passed; 0 failed`（2.62s）

---

## 6. 次タスクへの引き継ぎ

### タスク#8（取引CRUD）で必要な対応

`transactions.account_id` の FK は **`ON DELETE RESTRICT`** にする。`accounts.user_id` → `users(id)` が `ON DELETE CASCADE` なのと**意図的に非対称**にする。

| 関係 | 挙動 | 理由 |
|---|---|---|
| users → accounts | CASCADE | 退会したら口座も消えるのが妥当 |
| accounts → transactions | RESTRICT | 口座を消して取引履歴が消えるのは事故 |

RESTRICT にすれば、取引が紐づいた口座の DELETE は FK 違反（23503）となり、既存の `From<sqlx::Error>` が 422 に変換する。マイグレーション 0004 にこの意図をコメントで残しておく。

### タスク#7（domain::position）で使うもの

- `AccountType::is_tax_exempt()` — NISA つみたて / NISA 成長 / iDeCo が `true`
- `AccountType::holds_securities()` — `bank` のみ `false`

`bank` は現金残高のみを持つ口座という位置づけ。`/holdings` の集計対象から外すか、取引の入出金先として扱うかは、タスク#8で取引種別を決めるときに確定させる。

### 運用上の注意

- **`cargo sqlx prepare` の実行忘れ**が最大の落とし穴。クエリを1本足すたびに再実行が必要で、忘れると「ローカルの `cargo test` は通るが `docker compose build` だけ落ちる」という気付きにくい壊れ方をする。タスク#16の CI に `cargo sqlx prepare --check` を入れる。
- `tests/holdings_test.rs` は空だったため `holdings_test.rs.wip` に退避済み。タスク#9で戻す。
- `TestUser.id` は今回未使用（`#[allow(dead_code)]`）。他人のデータを直接 INSERT するテストで使う想定。

---

## 7. 実行コマンド一覧（再現用）

```bash
cd ~/workspace/shisan-api/asset-log

# --- 前提: .env に SQLX_OFFLINE を書かない、DATABASE_URL は localhost ---
# DATABASE_URL=postgres://assetlog:***@localhost:5432/assetlog
# PORT=8080

# スキーマ確認（ページャを切る）
docker compose exec db psql -U assetlog -d assetlog -P pager=off -c '\d accounts'
docker compose exec db psql -U assetlog -d assetlog -c '\dT+ account_type'

# 実装後の検証
cargo check                 # 実 DB に対してクエリを型検査
cargo sqlx prepare -- --all-targets
ls .sqlx | wc -l            # users 2 + accounts 5 = 7
git add .sqlx

# 統合テスト
cargo test --test accounts_test

# Docker ビルド（SQLX_OFFLINE は Dockerfile の ENV で与える）
cd ~/workspace/shisan-api
docker compose build api && docker compose up -d
curl -s localhost:8080/health
```

### 切り分けに使ったコマンド

```bash
# 環境変数がどこから来ているか
env | grep -i sqlx
cat .cargo/config.toml 2>&1

# .env の中身を制御文字込みで見る（macOS は cat -A が使えない）
sed -n '6p' .env | od -c

# ファイルが実際に存在し中身があるか
wc -l src/repository/account_repo.rs src/domain/account.rs
cat src/repository/mod.rs src/domain/mod.rs

# キャッシュを疑うとき
cargo clean -p asset-log && cargo check
```