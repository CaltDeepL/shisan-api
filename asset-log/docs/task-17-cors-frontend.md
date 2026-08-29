# タスク#17 CORS設定 + フロントエンド雛形

## 目的

バックエンド（タスク#1〜16）完了後、フロントエンド実装フェーズの起点となる作業。

- ブラウザから API を叩けるようにする（CORS）
- React SPA の雛形を立て、`/health` の疎通をブラウザ上で確認する

タスク#16（デプロイ）の②でコネクションプール調整を行った際、CORS は「フロント作業時に回す」として保留していた。その回収も兼ねる。

## 完了条件

| # | 条件 | 結果 |
|---|---|---|
| 1 | `CORS_ALLOWED_ORIGINS` で許可オリジンを外部設定できる | ✅ |
| 2 | プリフライト（OPTIONS）に CORS ヘッダが返る | ✅ |
| 3 | 許可外オリジンには CORS ヘッダが付かない | ✅ |
| 4 | 401 レスポンスにも CORS ヘッダが付く | ✅ |
| 5 | `web/` に Vite + React + TS 雛形が立ち、ブラウザから `/health` を表示できる | ✅ |
| 6 | `cargo test` 全 green・clippy・fmt パス | ✅（98 テスト） |

---

## 設計判断

### 別オリジン構成を採用した

| 案 | 構成 | CORS |
|---|---|---|
| **A（採用）** | Render Static Site（SPA）＋ Web Service（API） | 必要 |
| B | API が SPA を配信 / Render の rewrite | 不要 |

チェスアプリと同じ A 案。ポートフォリオとして「SPA と API が分離していて CORS を正しく扱える」ことを示せる。

### 開発時も Vite プロキシを使わない

`vite.config.ts` の `server.proxy` で `/api` を転送すれば開発中は同一オリジン扱いになり CORS が発生しないが、**採用しなかった**。

理由: 開発中は素通りして本番でだけ CORS エラーが出る、という最悪のパターンを踏みやすい。開発段階から素のクロスオリジンで動かし、設定ミスを早期に検知する。

### `allow_credentials` は付けない

認証は Cookie ではなく `Authorization: Bearer`（`AuthUser` 抽出子）のため不要。付けると `allow_origin` にワイルドカードが使えなくなる制約も背負う。

### `CorsLayer` は最外層に置く

```rust
router
    .merge(SwaggerUi::new("/docs").url("/openapi.json", api))
    .layer(cors::cors_layer(cors_origins))   // ← 最後
```

最外層に置くことで、Swagger UI・401 などのエラーレスポンス・404 まで全レスポンスに CORS ヘッダが乗る。

**特に 401 が重要。** ここにヘッダが付かないと、ブラウザ側では「認証エラー」ではなく「ネットワークエラー」として見え、原因究明が著しく難しくなる。統合テスト `unauthorized_response_has_cors_header` はこのレイヤ位置の検証を兼ねる。

### `CORS_ALLOWED_ORIGINS` 未設定はエラーにしない

`JWT_SECRET` は未設定なら起動失敗させているが、CORS は未設定でも起動させる。curl・サーバー間呼び出し・GitHub Actions からの `/snapshots/run` は CORS 無関係で動くため、起動を止める理由がない。

代わりに起動ログを `warn` で出し、Render のログで気付けるようにした。

```rust
if config.cors_allowed_origins.is_empty() {
    tracing::warn!("CORS_ALLOWED_ORIGINS が未設定です（ブラウザからのリクエストは拒否されます）");
} else {
    tracing::info!(origins = ?config.cors_allowed_origins, "CORS allowed origins");
}
```

### 値のバリデーションを `config.rs` で行う

スキーム必須・末尾スラッシュ除去を `parse_cors_origins()` に実装した。

ブラウザが送る `Origin` ヘッダは必ず `http://localhost:5173` の形（スキームあり・末尾スラッシュなし）。`.env` に `localhost:5173` や `http://localhost:5173/` と書くと文字列一致せず、**エラーは一切出ずに「CORS ヘッダが返らない」症状だけが残る**。ここで潰しておく。

### 許可オリジンは引数で渡す（`AppState` 経由にしない）

```rust
pub fn app(state: AppState, cors_origins: &[String]) -> Router
```

`AppState` が持つ `Config` から読む案もあったが、テストで許可オリジンを差し替えたいため引数にした。`AppState` 経由だとテストごとに `Config` を丸ごと組み立てる必要が出る。

呼び出し元は `main.rs` と `tests/common/mod.rs` の 2 箇所のみ。

### フロントを `web/` へ移動

当初はリポジトリルート直下に Vite プロジェクトが展開されていたが、`web/` サブディレクトリへ移動した。

| 論点 | ルート直下 | `web/`（採用） |
|---|---|---|
| `src/` の曖昧さ | ルートの `src/` と `asset-log/src/` が並存 | `web/src` と `asset-log/src` で対称 |
| CI の `paths` フィルタ | 除外条件が複雑化 | `asset-log/**` と `web/**` で素直に分離 |
| Render Static Site | Root Directory が `.` | Root Directory `web` と明示できる |

3 つ目が決め手。チェスアプリで Root Directory のずれに詰まった件と同じ地雷を避ける。移動コストは雛形段階の今が最小だった。

### openapi-typescript は依存に入れず `npx` で実行

`npm install -D openapi-typescript` が `ERESOLVE` で失敗した。openapi-typescript 7.13.0 のピア依存が `typescript@^5.x` 止まりで、プロジェクトの TypeScript 6 と衝突する。

型生成は「ビルド時に走るツール」ではなく「たまに手で叩いて `.d.ts` を吐かせるだけの道具」なので、依存に入れず `npx` で都度実行する方針にした。生成された型ファイルさえリポジトリに入れば本体は不要。

```json
"scripts": {
  "gen:api": "npx openapi-typescript ../asset-log/docs/openapi.json -o src/api/schema.d.ts"
}
```

実際の型生成はタスク#18（認証画面で API クライアントを書く時点）から使う。

---

## 詰まりどころ

### 1. curl でプリフライトが 405 → 古い Docker イメージだった

```
HTTP/1.1 405 Method Not Allowed
allow: POST,GET,HEAD
```

`CorsLayer` が入っていればプリフライトはレイヤが横取りするので 405 にはならない。405 は素の Axum ルータが「`/accounts` に OPTIONS ハンドラは無い」と答えた形で、`allow: POST,GET,HEAD` がルータ由来のヘッダ。

原因は**8080 で動いていたコンテナが変更前のバイナリだった**こと。

**教訓: Rust のコード変更は `docker compose restart` でも `--force-recreate` でも反映されない。`--build` が必須。**

```bash
docker compose up -d --build api
```

`--force-recreate` は環境変数の反映用であって、イメージの焼き直しはしない。

CORS は症状がブラウザ側にしか出ないため、「設定を直したのに直らない」と感じたら**まずビルドを疑う**のが早道。切り分けの順序としては、①統合テストが green か（green ならコードは正しい → 動いているバイナリを疑う）②起動ログに `CORS allowed origins` が出ているか、の 2 点で即座に判別できる。

### 2. `tsconfig.app.json` で `baseUrl` が非推奨エラー

```
オプション 'baseUrl' は非推奨であり、TypeScript 7.0 で機能しなくなります。
```

TypeScript 6 で `baseUrl` が非推奨化された。TS5 時代の記事にある `baseUrl` + `paths` の組み合わせは使えない。

**TS6 以降は `paths` を単独で書く。** `baseUrl` を省略すると `paths` は tsconfig ファイル自身の場所からの相対解決になるため、`./src/*` はそのまま意図どおりに効く。

```jsonc
"paths": { "@/*": ["./src/*"] },
```

`ignoreDeprecations: "6.0"` で黙らせる選択肢もあるが、移行猶予のための一時措置なので新規プロジェクトでは新しい書き方に合わせた。

### 3. Vite が 5174 で起動していた

```
Port 5173 is in use, trying another one...
➜  Local: http://localhost:5174/
```

前のターミナルで起動したままの Vite が 5173 を掴んでいた。**この状態でブラウザから叩くと Origin が `http://localhost:5174` になり、許可リストと一致せず CORS で弾かれる。**

`strictPort: true` を設定し、5173 が空いていなければ別ポートに逃げずに**起動エラーで止まる**ようにした。黙って別ポートで立ち上がって「なぜか CORS エラー」と悩むより、起動時点で気付ける方が良い。

```ts
server: { port: 5173, strictPort: true },
```

### 4. `__dirname` が将来使えなくなる警告

```
Your Vite config uses features that are unsupported by `configLoader: 'native'`
- `__dirname` (vite.config.ts:8:41). Use `import.meta.dirname` instead
```

`path.resolve(__dirname, "./src")` → `path.resolve(import.meta.dirname, "./src")` に置換。

---

## 変更ファイル

### バックエンド（asset-log）

| ファイル | 変更 |
|---|---|
| `Cargo.toml` | `tower-http` に `cors` feature 追加 |
| `src/config.rs` | `cors_allowed_origins: Vec<String>` 追加、`parse_cors_origins()` 新設、ユニットテスト 4 件 |
| `src/cors.rs` | 新規。`cors_layer()` |
| `src/lib.rs` | `pub mod cors;` 追加、`app()` のシグネチャ変更、最外層に `.layer()` |
| `src/main.rs` | `app()` 呼び出し変更、起動ログ追加 |
| `tests/common/mod.rs` | `test_app_with_cors()` 追加、`test_app()` をラッパ化 |
| `tests/cors_test.rs` | 新規。統合テスト 5 件 |

### インフラ・フロント

| ファイル | 変更 |
|---|---|
| `.env` / `.env.example` | `CORS_ALLOWED_ORIGINS` 追加 |
| `compose.yaml` | `api` の environment に `CORS_ALLOWED_ORIGINS` |
| `web/`（旧ルート直下） | Vite プロジェクトを移動 |
| `web/vite.config.ts` | Tailwind v4 プラグイン、`import.meta.dirname`、`strictPort` |
| `web/tsconfig.app.json` | `paths` 追加（`baseUrl` は書かない） |
| `web/.env.local` | `VITE_API_BASE_URL`（git 管理外） |
| `web/src/index.css` | `@import "tailwindcss";` |
| `web/src/App.tsx` | `/health` 疎通確認用の最小構成 |

### 導入ライブラリ（web）

`tailwindcss` / `@tailwindcss/vite` / `react-router` / `@tanstack/react-query` / `zustand`

---

## 申し送り

### Render の `CORS_ALLOWED_ORIGINS` は未設定

Static Site の URL が確定するのはタスク#24 なので、現時点では未設定が正しい状態。

- 設定タイミング: タスク#24 で Static Site をデプロイした直後
- 値の形式: スキーム必須・末尾スラッシュなし・カンマ区切り
- 例: `https://shisan-web.onrender.com`
- 設定後は Web Service の再デプロイが必要な場合がある

**忘れると本番でだけ動かない典型的な事故になる。**

### フロント用 CI は未整備

既存の `ci.yml` は `paths` フィルタで `asset-log/` 配下の変更時のみ発火する。`web/` 配下だけを変更した PR では CI が走らず、`workflow_run` で連鎖する Deploy も動かない。

フロント用のジョブ（型チェック + ビルド）は別ワークフローとして分けるのが無難。タスク#24 で対応する。

### `VITE_` 環境変数はブラウザから見える

`VITE_` プレフィックスの環境変数は**ビルド成果物にそのまま埋め込まれる**。API キーやシークレットは置けない。バックエンドの `.env` とは性質が異なる。

---

## 再現コマンド

### バックエンドの検証

```bash
cd asset-log

# テスト
cargo test --test cors_test        # 5 件
cargo test                          # 98 件
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

```bash
# 起動（コード変更時は --build 必須）
cd ~/workspace/shisan-api
docker compose up -d --build api
docker compose logs api | grep -i cors
# → INFO asset_log: CORS allowed origins origins=["http://localhost:5173"]
```

```bash
# プリフライト
curl -i -X OPTIONS http://localhost:8080/accounts \
  -H "Origin: http://localhost:5173" \
  -H "Access-Control-Request-Method: POST" \
  -H "Access-Control-Request-Headers: authorization,content-type"
```

期待される応答:

| ヘッダ | 値 |
|---|---|
| ステータス | `200 OK` |
| `access-control-allow-origin` | `http://localhost:5173` |
| `access-control-allow-headers` | `authorization,content-type` |
| `access-control-allow-methods` | `GET,POST,PATCH,DELETE` |
| `access-control-max-age` | `600` |
| `vary` | `origin, access-control-request-method, access-control-request-headers` |

```bash
# 許可オリジン
curl -i http://localhost:8080/health -H "Origin: http://localhost:5173"
# → access-control-allow-origin あり

# 許可外オリジン
curl -i http://localhost:8080/health -H "Origin: https://evil.example"
# → 200 OK で本文は返るが access-control-allow-origin なし
```

**CORS はサーバーが拒否する仕組みではない。** サーバーは「許可の証明書を出すか出さないか」を決めるだけで、実際に応答を捨てるのはブラウザ。curl では常に成功する。

### フロントエンドの検証

```bash
cd web
npm run dev
# → http://localhost:5173/ （5174 等になっていたら strictPort が効いていない）
```

ブラウザで http://localhost:5173 を開き、

1. `API status: ok` と表示されること
2. DevTools → Network → `health` → Response Headers に
   `access-control-allow-origin: http://localhost:5173` が乗っていること

**2 が本当の完了条件。** curl で通ることと、ブラウザの fetch で通ることは別の確認。