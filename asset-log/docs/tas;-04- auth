# タスク#4: 認証（register / login / JWT）

## 1. ゴールと完了条件

| # | 完了条件 | 結果 |
|---|---|---|
| 1 | `POST /auth/register` が 201 + JWT、重複メールは 409 | ✅ |
| 2 | `POST /auth/login` が 200 + JWT、資格情報の誤りは 401 | ✅ |
| 3 | 保護API（`GET /me`）がトークン無し・不正・期限切れで 401 | ✅ |
| 4 | 401 に `WWW-Authenticate: Bearer` が付く | ✅ |
| 5 | パスワードは Argon2id でハッシュ化され、平文もハッシュも応答に出ない | ✅ |
| 6 | ログイン失敗時、メールの登録有無が応答内容からも応答時間からも分からない | ✅ |

---

## 2. 実装内容

### 追加した依存

```toml
argon2 = "0.5"
jsonwebtoken = "9"
axum-extra = { version = "0.10", features = ["typed-header"] }
chrono = { version = "0.4", features = ["serde"] }
rand = "0.8"
dotenvy = "0.15"
```

### 追加・変更したファイル

| パス | 内容 |
|---|---|
| `src/auth/mod.rs` | `jwt` / `password` の宣言 |
| `src/auth/password.rs` | Argon2id のハッシュ化・検証、タイミング均一化用のダミー検証 |
| `src/auth/jwt.rs` | `Claims`、`JwtKeys`（発行・検証） |
| `src/middleware/auth.rs` | `AuthUser` extractor（`FromRequestParts` 実装） |
| `src/repository/user_repository.rs` | `insert` / `find_credentials_by_email` |
| `src/handler/auth.rs` | `register` / `login` / `me`、入力バリデーション |
| `src/state.rs` | `AppState` に `jwt` を追加、`FromRef` 実装 |
| `src/config.rs` | `JWT_SECRET` / `JWT_TTL_MINUTES` の読み込みと起動時チェック |
| `src/error.rs` | `InvalidCredentials` 追加、401 に `WWW-Authenticate` |
| `src/main.rs` | ルーティング追加、`_debug` エンドポイント削除 |

---

## 3. 設計判断の根拠

### 3.1 middleware ではなく extractor（`AuthUser`）を採用

ロードマップの表記は「JWT middleware」だったが、`axum::middleware::from_fn` + `Extension<UserId>` 方式には穴がある。ハンドラ側で `Extension` を取り忘れてもコンパイルが通ってしまい、認証が効いているかどうかが型に現れない。

`FromRequestParts` を実装した `AuthUser` なら、引数に書いた時点で認証が必須になり、書き忘れは「user_id が取れない」＝コンパイルエラーになる。タスク#5以降で `user_id` によるスコープ制限を入れるとき、この差が効く。

グループ単位で塞ぎたくなったら `route_layer` を併用できるので、後戻りもしない。

### 3.2 Argon2 は `spawn_blocking` に載せる

Argon2id はデフォルトパラメータで数十〜100ms、メモリを 19MiB 使う。tokio のワーカースレッド上で直接回すと、その間そのスレッドは他のリクエストを処理できない。ログインが同時に数本来ただけで全体のレイテンシが跳ねる。

### 3.3 ログイン失敗時はユーザー不在でも必ずハッシュ検証を空回しする

メールが存在しないときに即 401 を返すと、応答時間の差で登録の有無が判別できる（ユーザー列挙攻撃）。`DUMMY_HASH` に対して検証を走らせて時間を揃える。

メッセージも「メールアドレスまたはパスワードが違います」に統一し、どちらが誤りか漏らさない。

### 3.4 `Unauthorized` と `InvalidCredentials` を分ける

どちらも 401 だが、意味が違う。

- `Unauthorized`: トークンが無い・壊れている・期限切れ → クライアントは「ログイン画面へ」
- `InvalidCredentials`: ログイン時の資格情報の誤り → クライアントは「入力し直して」

RFC 9457 の `type` フィールドが `/errors/unauthorized` と `/errors/invalid-credentials` に分かれるので、フロント側は文言に依存せず分岐できる。ステータスコードは同じなのでメールの存在有無は漏れない。

### 3.5 `login` では入力バリデーションを通さない

`register` では「パスワードは12文字以上」を 422 で返すが、`login` では返さない。返してしまうと、登録済みパスワードが満たすべき条件を攻撃者に教えることになる。`login` は「一致するかしないか」だけを見る。

### 3.6 鍵は `AppState` に持たせる

`EncodingKey::from_secret` をリクエストごとに構築するのは無駄なので、起動時に1回だけ作って使い回す。`FromRef<AppState> for JwtKeys` を実装しておけば、extractor 側は `AppState` の具体型に依存しない。

### 3.7 `JWT_SECRET` は起動時に検証して落とす

未設定または32バイト未満なら `Config::from_env()` で `Err` を返し、`main` で panic させる。実行中に初めて気づくより、コンテナが起動しない方が安全。

### 3.8 リフレッシュトークンは今回やらない

TTL を短くすると再ログインが頻発し、長くすると失効できない、というトレードオフがあるが、これは失効ストア（DB か Redis）の設計込みの話になる。まずはアクセストークン単体（TTL 60分）で完了条件を満たし、拡張として後回しにする。

### 3.9 `DUMMY_HASH` は起動時に温める

`LazyLock` のままだと、未登録メールでの**初回**ログインだけ「ダミーハッシュの生成 + 検証」で Argon2 が2回走り、応答時間が約2倍になる。実測でも初回のみ 0.45s、以降 0.23s だった。

サーバー起動時に一度 `warmup()` を呼んで初期化を済ませておく。`spawn_blocking` に載せるのは、起動時とはいえランタイムのワーカーを100ms止めないため。

```rust
tokio::task::spawn_blocking(auth::password::warmup)
    .await
    .expect("warmup task panicked");
```

### 3.10 `.env` を2本立てにする

ホストから `cargo run` するときと、コンテナで動かすときで `DATABASE_URL` のホスト名が違う（`localhost` vs `db`）。1つのファイルで両立できないので分ける。

| ファイル | 用途 | `DATABASE_URL` のホスト |
|---|---|---|
| `shisan-api/.env` | Compose が読む（コンテナへ渡す） | `db` |
| `asset-log/.env` | `dotenvy` が読む（ホスト実行時） | `localhost` |

`dotenvy::dotenv()` は `.env` が無い環境ではエラーにしないので、コンテナ内でも同じバイナリがそのまま動く（compose の `environment` が使われる）。

```rust
// ホストから直接起動するとき用。コンテナでは compose の environment が使われる
let _ = dotenvy::dotenv();
```

### 3.11 `_debug` エンドポイントを削除

タスク#3で作った `/_debug/conflict` と `/_debug/check` は検証済みなので削除。残しておくと本番で誰でもレコードを作れる穴になる。再現が必要になれば Git 履歴から拾える。

---

## 4. つまずいた点と教訓

### 4.1 `mod.rs` を作ったつもりで0行だった

`cat > auth/mod.rs <<'EOF' ... EOF` を実行したのに、`wc -l` が 0 を返した。ディレクトリを取り違えていたか、途中で中断されたか。

エディタのツリーにファイルが「見える」ことと、中身があることは別。`mod` 系のエラーが出たら、まず `wc -l src/*/mod.rs` で一括確認する。

```
       2 src/auth/mod.rs
       0 src/domain/mod.rs      ← まだ何も無いので0でOK
       6 src/handler/mod.rs
       0 src/middleware/mod.rs  ← auth.rs があるのに0 = 原因
```

**教訓**: `E0583 file not found for module` と `E0432 could not find X in Y` は別物。前者はファイルが無い、後者はファイルはあるが `mod` 宣言が無い。

### 4.2 `>>` のつもりが既存内容を消していた

`echo 'pub mod auth;' >> handler/mod.rs` で追記したはずが、結果的に `handler/mod.rs` が1行だけになっていた。他のハンドラの宣言が消えた状態。

`>>` と `>` の取り違えはリカバリが効かない（Git 管理下ならまだしも、未追跡ファイルだと戻せない）。ヒアドキュメントで全体を書き直す方が、意図が明示されて安全。

### 4.3 提示されたコードを「貼る場所」を間違えた

`OnConstraint` の呼び出し例（`handler/auth.rs` で使うもの）を、`error.rs` のトレイト実装の中に貼ってしまい、こうなった。

```
error: expected one of `...`, `..=`, `..`, `:`, or `|`, found `,`
163 |     fn on_constraint("users_email_lower_key", || {
```

関数定義の引数位置に文字列リテラルが来ている、というエラー。**構文エラーの内容から「そもそも定義と呼び出しを取り違えている」と読める**ようになると、この種の混乱は早く抜けられる。

同様に、`error.rs` の `IntoResponse` に「1行足すだけ」と言われた `headers.insert(...)` も、実際には `headers` という変数が存在しなかった。差分を貼る前に、周囲のコードで変数が定義されているか確認する。

### 4.4 `DATABASE_URL` のホスト名（再発）

タスク#2で踏んだのと同じ。`query_as!` / `query_scalar!` はマクロ展開時に実 DB へ接続するため、`cargo check` の時点で `DATABASE_URL` が要る。`.env` の値は `db:5432`（コンテナ用）なので、ホストからは解決できない。

```
error: error communicating with database: failed to lookup address information
```

| 実行場所 | ホスト名 |
|---|---|
| コンテナ内（api / tools） | `db` |
| macOS のシェル | `localhost` |

**恒久対策**: `.sqlx/` をコミットし、`SQLX_OFFLINE=true` を効かせる。クエリマクロを追加したら `cargo sqlx prepare` をセットで実行する運用にする。

### 4.5 `.env` を `source` できなかった

```
.env:9: parse error near `\n'
```

zsh はファイル全体をパースしてから実行するので、9行目のエラーで1行目も実行されない。結果、`POSTGRES_PASSWORD` が空のまま接続文字列を組み立てて `password authentication failed` になった。

`.env` は Compose 用の設定ファイルであってシェルスクリプトではないため、Compose では通るが zsh では通らない書き方がある（行末のバックスラッシュ、引用符の閉じ忘れ、値に含まれる `#` や空白など）。値をシングルクォートで囲んでおけば両方で通る。

**確認方法**: `echo "${#POSTGRES_PASSWORD}"` で桁数だけ見る。0 なら読めていない。

### 4.6 メモに書いた `***` をそのまま打った

前回の作業メモで接続文字列のパスワードを `***` に伏せていたが、それを実際の値と思って実行し `password authentication failed` になった。

**教訓**: メモに伏せ字を書くときは `<パスワード>` のように、明らかにプレースホルダと分かる形にする。`***` は値としても成立しうるので紛らわしい。

### 4.7 macOS と GNU のコマンド差異

- `cat -A` は GNU 版のオプション。BSD（macOS）では `cat -et`
- `sed -i` は macOS では空文字列の引数が必要（`sed -i '' 's/.../.../'`）

### 4.8 `libpq` は keg-only

`brew install libpq` してもコマンドは PATH に入らない。`/opt/homebrew/opt/libpq/bin`（Apple Silicon）を `~/.zshrc` に追加する必要がある。

`postgresql@N` ではなく `libpq` を選ぶのは、ホスト側でサーバーを起動させないため。サーバーまで入れると 5432 番をコンテナと奪い合う。

### 4.9 ポート衝突（再発）

```
failed to bind port: Os { code: 48, kind: AddrInUse }
```

Compose の `api` コンテナが 8080 を掴んだまま `cargo run` した。ホストから動かす間は `docker compose stop api`（`db` は残す）。

犯人の特定は `lsof -nP -iTCP:8080 -sTCP:LISTEN`。

### 4.10 `dotenvy` は既存の環境変数を上書きしない

`asset-log/.env` を用意したのに、シェルに残っていた `DATABASE_URL`（`db:5432`）が優先され続けた。`dotenvy::dotenv()` は「未設定の変数だけ」を埋める仕様。

優先順位は **シェルの環境変数 > `.env`** なので、`.env` を直したのに反映されないときは、まず `echo "$VAR"` でシェル側を疑う。

```bash
echo "$DATABASE_URL"   # 何か出たら .env は無視されている
unset DATABASE_URL
```

そもそも `set -a; source .env; set +a` する運用は、`sqlx-cli` を使うために始めたもの。`asset-log/.env` を整えた今は不要になったので、以後は `source` しない。**新しいターミナルで `cd asset-log && cargo run` だけで起動する**状態がゴール。

### 4.11 ルートの `.env` をコピーして流用した

`asset-log/.env` を作るとき、ルートの `.env` をそのままコピーしたため `DATABASE_URL` が `db:5432` のままだった。`PGADMIN_*` など Rust 側が読まない変数も混入していた。

2つのファイルは目的が違うので、コピーではなく必要な変数だけ書く。

### 4.12 `Cargo.toml` に書く行をターミナルに打った

`dotenvy = "0.15"` を `zsh` に貼って `command not found` になった。依存の追加は `cargo add dotenvy` が確実（`Cargo.toml` の書式を間違えることもない）。

### 4.13 `AppError::field()` と `FieldError` の混同

`AppError::field()` は「単独のバリデーションエラーを持つ `AppError`」を返すヘルパーであって、`FieldError` を返すわけではない。`Vec` に積む用途には使えず、こうなった。

```
expected `Vec<FieldError>`, found `Vec<AppError>`
```

`FieldError::new()` を追加して用途を分けたが、結果として似た名前の関数が2つ残った。タスク#5に入る前にどちらかへ寄せるか、`AppError::field()` を削除するか判断する。

---

## 5. 検証結果

### 完了条件1: 登録と重複

```
$ curl -i -X POST localhost:8080/auth/register \
    -d '{"email":"jis@example.com","password":"correct-horse-battery"}'
HTTP/1.1 201 Created
{"access_token":"eyJ0eXAiOiJKV1Qi...","token_type":"Bearer","expires_in":3600}

$ curl -i -X POST localhost:8080/auth/register \
    -d '{"email":"JIS@example.com","password":"correct-horse-battery"}'
HTTP/1.1 409 Conflict
{"type":"/errors/conflict","detail":"このメールアドレスは既に登録されています",...}
```

大文字小文字の違いで弾けている＝ `users_email_lower_key`（`lower(email)` のユニークインデックス）が効いている。

### 完了条件2・3・4: 認証

```
$ curl -i localhost:8080/me
HTTP/1.1 401 Unauthorized
www-authenticate: Bearer realm="asset-log"
{"type":"/errors/unauthorized",...}

$ curl -i localhost:8080/me -H "Authorization: Bearer not.a.jwt"
HTTP/1.1 401 Unauthorized

$ curl -i localhost:8080/me -H "Authorization: Bearer $TOKEN"
HTTP/1.1 200 OK
{"user_id":"93778b9b-49df-4c5a-bd11-19b22ad1c988"}
```

`/me` が返した `user_id` が register 時の JWT の `sub` と一致 → トークンからユーザーが正しく復元できている。

期限切れは `JWT_TTL_MINUTES=0` で起動し、発行後に `leeway`（5秒）を超えて待ってから確認。

```
$ sleep 7 && curl -i localhost:8080/me -H "Authorization: Bearer $T"
HTTP/1.1 401 Unauthorized
```

ログイン失敗は `type` が分岐している。

```
$ curl -s -X POST localhost:8080/auth/login \
    -d '{"email":"jis@example.com","password":"wrong-password"}' | jq .type
"/errors/invalid-credentials"
```

### 完了条件6: タイミング

| ケース | warmup 導入前 | warmup 導入後 |
|---|---|---|
| 登録済みメール + 誤パスワード | 0.239s | 0.23s |
| 未登録メール（1回目） | 0.45s | 0.25s |
| 未登録メール（2回目以降） | 0.23s | 0.23s |

初回だけ約2倍になっていたのは、`DUMMY_HASH` が `LazyLock` のため Argon2 が2回走っていたため（3.9 参照）。起動時の `warmup()` で解消し、**1回目から登録済みメールと同じ 0.23s 台**に揃った。

---

## 6. 次タスクへの引き継ぎ

### タスク#5（口座CRUD）で使うもの

```rust
pub async fn list_accounts(
    AuthUser(user_id): AuthUser,     // ← これを書けば認証が必須になる
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Account>>> { ... }
```

`user_id` を必ず `WHERE user_id = $1` に含めること。含め忘れると他人の口座が見える。「取得できたが自分のものではない」ケースは 404（存在を隠す）か 403（存在は認める）かの判断が要る。一般には 404 が安全。

`accounts_user_name_key` はタスク#3で 409 にマップ済みなので、口座名の重複はハンドラ側に分岐を書かずに済む。

### 未消化・要判断の項目

- **`AppError::field()` と `FieldError::new()` の重複**: どちらかに寄せる
- **`verify_dummy()` と `warmup()` の重複**: `warmup()` を追加したので `verify_dummy` は login からのみ呼ばれる。役割は分かれているが、`password.rs` の公開関数が4つになったので整理の余地あり
- **`BadRequest` / `Forbidden` の dead_code 警告**: タスク#5で使われる想定
- **リフレッシュトークン**: 3.8 の通り、失効ストアの設計込みで後回し
- **レート制限**: `login` に総当たり対策が無い。tower-governor などをタスク#16前後で検討
- **`user_id` のスコープ漏れを防ぐ仕組み**: 手動で `WHERE user_id` を書く運用は事故りやすい。リポジトリ層のシグネチャに `user_id` を必須引数として入れる案

### Google ログイン（OIDC）について

`JwtKeys` と `AuthUser` の土台ができたので、OIDC は「コールバックで受け取った ID トークンを検証し、`users` に upsert して自前 JWT を発行する」だけで乗る。ロードマップ通り、タスク#16の後の拡張で良い。

---

## 7. 実行コマンド一覧（再現用）

```bash
# --- ホストから開発サーバーを起動する ---
cd ~/workspace/shisan-api
docker compose stop api            # 8080 を空ける（db は残す）

cd asset-log
cargo run                          # asset-log/.env を dotenvy が読む

# --- クエリマクロを追加・変更したら ---
SQLX_OFFLINE=false cargo sqlx prepare -- --all-targets
git add .sqlx

# --- 検証 ---
curl -i localhost:8080/me
curl -i localhost:8080/me -H "Authorization: Bearer not.a.jwt"

curl -i -X POST localhost:8080/auth/register \
  -H 'content-type: application/json' \
  -d '{"email":"jis@example.com","password":"correct-horse-battery"}'

TOKEN=$(curl -s -X POST localhost:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"jis@example.com","password":"correct-horse-battery"}' | jq -r .access_token)

curl -i localhost:8080/me -H "Authorization: Bearer $TOKEN"

# 期限切れの確認
JWT_TTL_MINUTES=0 cargo run   # 発行後 7 秒待って /me → 401

# タイミングの確認（1回目から 0.23s 前後なら warmup が効いている）
for i in 1 2 3; do
  /usr/bin/time -p curl -s -o /dev/null -X POST localhost:8080/auth/login \
    -H 'content-type: application/json' \
    -d '{"email":"nobody@example.com","password":"wrong-password"}' 2>&1 | grep real
done

# --- 後片付け ---
docker compose exec db psql -U assetlog -d assetlog \
  -c "DELETE FROM users WHERE email = 'jis@example.com'"

cd ~/workspace/shisan-api
docker compose up --build -d

# --- トラブルシュート ---
lsof -nP -iTCP:8080 -sTCP:LISTEN   # ポート衝突の犯人（kill <PID>）
wc -l src/*/mod.rs                 # mod.rs の空ファイル検出
echo "$DATABASE_URL"               # 何か出たら .env が無視されている → unset
grep -o '^[A-Z_]*' .env            # .env に何の変数があるか
```

`psql` を使う場合は PATH を通しておく（`libpq` は keg-only）。

```bash
export PATH="/opt/homebrew/opt/libpq/bin:$PATH"   # ~/.zshrc に追記済み
```

## 8. 環境変数

`.env` は2箇所にあり、役割が違う（3.10 参照）。

### `shisan-api/.env`（Compose 用）

```
POSTGRES_USER=assetlog
POSTGRES_PASSWORD=<パスワード>
POSTGRES_DB=assetlog
DATABASE_URL=postgres://assetlog:<パスワード>@db:5432/assetlog
PORT=8080
RUST_LOG=info
JWT_SECRET='<openssl rand -base64 48 の出力を1行で>'
JWT_TTL_MINUTES=60
PGADMIN_DEFAULT_EMAIL=admin@example.com
PGADMIN_DEFAULT_PASSWORD=<パスワード>
```

### `asset-log/.env`（ホストから `cargo run` する用）

```
DATABASE_URL=postgres://assetlog:<パスワード>@localhost:5432/assetlog
PORT=8080
RUST_LOG=info
SQLX_OFFLINE=true
JWT_SECRET='<同じ値でよい>'
JWT_TTL_MINUTES=60
```

`compose.yaml` の `api` サービスに `JWT_SECRET: ${JWT_SECRET}` と `JWT_TTL_MINUTES: ${JWT_TTL_MINUTES}` を追加すること。

両方の `.env` が `.gitignore` されているか確認する。

```bash
git check-ignore -v .env asset-log/.env
```