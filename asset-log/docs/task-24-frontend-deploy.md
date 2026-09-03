# タスク #24: Render Static Site へのフロントエンドデプロイ

## 概要

`web/` の React SPA を Render Static Site として本番公開し、既存の
`shisan-api`（Render Web Service）と接続した。

| 項目 | 値 |
|---|---|
| 本番フロント | https://shisan-web.onrender.com |
| 本番API | https://shisan-api.onrender.com |
| デプロイ方式 | CI green → Deploy Hook（バックエンドと統一） |

## 構成

### Render Static Site（shisan-web）

| 設定 | 値 |
|---|---|
| Root Directory | `web` |
| Build Command | `npm ci && npm run build` |
| Publish Directory | `dist` |
| Auto-Deploy | Off |
| Redirects/Rewrites | `/*` → `/index.html`（Rewrite） |
| 環境変数 | `VITE_API_BASE_URL=https://shisan-api.onrender.com` |

Root Directory を設定すると Build Command / Publish Directory はそこからの
相対になるため、Publish Directory は `web/dist` ではなく `dist`。

Rewrite ルールはリソースが存在しないパスにのみ適用されるため、
`/assets/*.js` などのバンドルがワイルドカードに食われることはない。

### GitHub Actions

既存の `ci.yml` / `deploy.yml` は変更せず、web 専用に2本を追加した。

| ワークフロー | トリガー | 役割 |
|---|---|---|
| `ci.yml` | `paths: asset-log/**` | Rust の CI |
| `deploy.yml` | `workflow_run: CI` | API の Deploy Hook |
| `ci-web.yml` | `paths: web/**` | `npm run lint` / `check:schema` / `build` |
| `deploy-web.yml` | `workflow_run: CI (web)` | Static Site の Deploy Hook |

CI と Deploy が同じ paths 条件を共有するため、#16 で踏んだ
「paths フィルタで CI がスキップされると Deploy も連鎖しない」問題が
構造的に発生しない。

`workflow_run` はワークフローが main に載って以降しか発火しないため、
初回は `workflow_dispatch` で手動実行する。

### CORS

バックエンドの `CORS_ALLOWED_ORIGINS` に本番フロントのオリジンのみを設定。
ローカル開発はローカルAPI（`localhost:8080`）を叩くため、本番APIの
許可リストに localhost は含めない。

## つまずいた点

### Render の onrender.com サブドメインはリネームで変わらない

サブドメインはサービス作成時に確定し、その後サービス名を変更しても追随しない。
名前を `shisan-api-1` → `shisan-web` に変えたが URL は
`shisan-api-1.onrender.com` のままで、`shisan-web.onrender.com` は
どのサービスにも紐づかない状態だった。URL を変えたい場合はサービスの
作り直しが必要。

このとき返ってきたヘッダが手がかりになる。

    x-render-routing: no-server

これは「そのホストに配信元が存在しない」を意味し、ファイルが見つからない
場合の 404 とは別物。Render で 404 を追うときは、まずサービス詳細画面に
表示されている実際の URL を確認する。サービス名から URL を推測すると
診断を誤る。

### Auto-Deploy を off で作成すると初回デプロイも走らない

CI green → Deploy Hook 方式にするため Auto-Deploy を off にして作成した結果、
一度もデプロイが走らず全パスが 404 になった。Manual Deploy で解消。

### Vite の環境変数はビルド時に焼き込まれる

`import.meta.env.VITE_*` はバンドルに埋め込まれるため、Render の Environment を
後から変更しても再ビルドするまで反映されない。初回ビルド前に設定しておく必要がある。

対策として `client.ts` の先頭にガードを置いた。

```ts
if (!BASE) throw new Error("VITE_API_BASE_URL が未設定のままビルドされています");
```

`undefined/auth/login` へリクエストが飛ぶ状態を、画面を触る前に検出できる。

### 環境変数の変更が実インスタンスに届いていなかった

`CORS_ALLOWED_ORIGINS` をダッシュボードで設定した後もアプリ側が空のまま起動しており、
ブラウザからのリクエストが全て CORS で落ちた。Manual Deploy で解消。
起動ログで確認できる。

    INFO asset_log: CORS allowed origins origins=["https://shisan-web.onrender.com"]

外から確認する場合はプリフライトを直接叩く。`access-control-allow-origin` が
返っていなければ許可リストに入っていない。

```bash
curl -i -X OPTIONS https://shisan-api.onrender.com/auth/login \
  -H "Origin: https://shisan-web.onrender.com" \
  -H "Access-Control-Request-Method: POST" \
  -H "Access-Control-Request-Headers: content-type"
```

### 空のシークレットを curl に渡すと exit 3

`RENDER_DEPLOY_HOOK_WEB` が未設定の状態で Deploy web を実行したところ、

    curl: (3) URL rejected: Malformed input to a URL function

ログ上はシークレットが `***` とマスクされるため、空かどうかを見分けられない。
明示的な存在チェックを入れると原因が特定しやすい。

### .env の POSTGRES_PASSWORD を変えても既存ボリュームは変わらない

パスワードは DB の初期化時に永続化されるため、`.env` を書き換えても
既存ボリュームには反映されない。変更するには `docker compose down -v` が必要。

## 副次的な修正

### .env.example に CORS_ALLOWED_ORIGINS が欠落していた

CORS 機能の追加時に `.env.example` を更新しておらず、README 通りに
`cp .env.example .env` でセットアップすると許可オリジンが空になり、
フロントからのリクエストが全て拒否される状態だった。

空でも起動はするため気づきにくい。起動時の warn を追加した。
出力は `main.rs`（起動シーケンス）側に置いている。`config.rs` は
`Config::from_env()` で値をパースするだけで、ログ出力の責務は持たない。

```rust
// asset-log/src/main.rs
tracing::warn!(
    "CORS_ALLOWED_ORIGINS が未設定です（ブラウザからのリクエストは拒否されます）"
);
```

この warn は追加直後に実際に役立った。ダッシュボードに値が入っているのに
アプリに届いていない状態を、起動ログだけで特定できた。

検証として `.env` を退避してから `.env.example` 由来で再セットアップし、
プリフライト・登録・ログインまで通ることを確認している。

### Node を 24 に移行

ローカルが Homebrew の Node 25 だったが、奇数系は LTS にならないため
24（Active LTS、EOL 2028-04-30）へ移行。`web/.node-version` でローカル・CI・
Render の3か所を揃えた。バージョン管理は fnm（`--use-on-cd` で `.node-version` に追従）。

移行のきっかけは Homebrew の依存ずれで node 25.8.2 が
`libsimdjson.31.dylib` を見つけられず起動不能になったこと。

## 検証項目

| # | 確認 | 結果 |
|---|---|---|
| 1 | 公開URLでログイン画面が表示される | OK |
| 2 | 新規登録 → ログイン（CORSエラーなし） | OK |
| 3 | `/holdings` を直リンクで開いて 404 にならない | OK |
| 4 | `/analytics` 表示中にリロードして 404 にならない | OK |
| 5 | `/assets/index-*.js` が Rewrite に食われず 200 | OK |
| 6 | 401 で自動ログアウトする | OK |
| 7 | main へのマージでフロントが自動デプロイされる | OK |

3〜5 は Rewrite ルールの検証。「存在しないパスだけが index.html に落ちる」
という意図どおりの挙動を確認している。

## 残課題

| 項目 | 内容 |
|---|---|
| コード分割 | バンドル 742KB / gzip 214KB。Vite が 500KB 超で警告中 |
| `readProblem` | `as ProblemDetails` 決め打ち。problem+json でない本文を raw に分離したい |
| ダイアログ開閉パターン | accounts系（nullable prop + useEffect）と assets/transactions系（key remount + mount-once useEffect）が混在。統一には親ページの条件付きレンダリング変更が必要 |
| コールドスタート | 無料インスタンスのスピンダウンで初回ログインが50秒以上待たされる。ローディング表現の検討余地あり |