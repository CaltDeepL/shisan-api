# タスク#18 認証画面（register / login / 401自動ログアウト）

## 目的

タスク#17 で立てたフロントエンド雛形の上に、認証周りの土台を作る。

バックエンドの `POST /auth/register` / `POST /auth/login` / `GET /me` は #4 で完成済み。ここで作るのは「トークンをどう持つか」と「401 をどう扱うか」であり、以降の #19〜#23 の全画面がこの上に乗る。API クライアントとルーティングの骨格を決める回でもある。

## 完了条件

| # | 条件 | 結果 |
|---|---|---|
| 1 | `/register` から登録でき、そのままログイン状態になる | ✅ |
| 2 | `/login` でトークンを取得・保持し、リロードしても維持される | ✅ |
| 3 | 未認証で保護ページを開くと `/login` へ飛び、ログイン後に元のページへ戻る | ✅ |
| 4 | API が 401 を返したら自動ログアウトして `/login` へ | ✅ |
| 5 | バリデーションエラー（12文字未満、メール重複）が画面に日本語で出る | ✅ |
| 6 | `tsc -b` と `npm run build` が通る | ✅ |

---

## 追加・変更したファイル

```
web/src/
├── api/
│   ├── schema.d.ts      # openapi-typescript 生成物（コミット対象）
│   ├── problem.ts       # ProblemDetails 型、ApiError クラス
│   ├── client.ts        # apiFetch、トークン注入、AUTH_EXPIRED 発火
│   └── auth.ts          # register / login / fetchMe
├── stores/
│   └── auth.ts          # zustand + persist、currentToken、initAuth
├── lib/
│   └── queryClient.ts   # TanStack Query の既定値（retry 分岐）
├── routes/
│   ├── RequireAuth.tsx  # 保護領域のガード
│   ├── GuestOnly.tsx    # 認証済みをログイン画面から弾く＋復帰先判定
│   └── AppLayout.tsx    # ヘッダー付き外枠
├── components/
│   ├── Field.tsx        # ラベル＋入力＋エラー表示
│   └── FormError.tsx    # フィールドに紐づかないエラー
├── pages/
│   ├── LoginPage.tsx
│   ├── RegisterPage.tsx
│   └── DashboardPage.tsx  # GET /me を叩くだけの仮ページ（#19 で置換）
├── App.tsx              # ルーティング（#17 の疎通確認用から全面差し替え）
└── main.tsx             # initAuth() を createRoot より前に呼ぶ
```

---

## 設計判断

### 1. トークンは localStorage に置く

| 案 | 挙動 | 判断 |
|---|---|---|
| メモリのみ | リロードで毎回ログイン | 安全だが開発・デモの体験が悪い |
| sessionStorage | タブを閉じると消える | XSS 耐性は localStorage と同じ |
| **localStorage（採用）** | 期限までログイン維持 | XSS を踏めば奪われる |

本来の正解は httpOnly Cookie + リフレッシュトークンだが、それはバックエンドの認証方式そのものの変更（Cookie 発行、CSRF 対策、別オリジンでの `SameSite=None; Secure`）を伴い、#18 の範囲を超える。

**リフレッシュトークンを持たない構成なので、漏洩時の被害は有効期限内に限定される。** Google ログイン（OIDC）を入れる段階で Cookie 方式ごと再検討する。→ Future Work

### 2. 同じ 401 でも「トークンを付けたか」で挙動を分ける

完了条件4を素直に実装すると事故が起きる。**ログインのパスワード間違いも 401 で返る**ため、自動ログアウトが発火して、エラーメッセージが表示される前に画面が飛ぶ。

判定基準は「そのリクエストに `Authorization` を付けたかどうか」。

```ts
// client.ts
if (res.status === 401 && token) {
  window.dispatchEvent(new Event(AUTH_EXPIRED));
}
```

| 経路 | token | 401 の意味 | 挙動 |
|---|---|---|---|
| `POST /auth/login` | なし（`auth: false`） | 認証情報が違う | フォームにエラー表示 |
| `GET /me` など | あり | 期限切れ・無効 | 自動ログアウト → `/login` |

バックエンド側も、`login` では `validate` を通さず「12文字未満です」を返さない設計（登録済みパスワードの条件を漏らさないため）。フロントもこれに合わせ、ログイン画面にはフィールド単位のバリデーションを置いていない。

### 3. `expires_in` は絶対時刻に変換して保存する

`expires_in` は「受け取った時点からの残り秒数」。そのまま localStorage に入れると、3日後にリロードしても「あと1時間ある」と誤判定する。

```ts
setSession: (res) => set({
  token: res.access_token,
  expiresAt: Date.now() + res.expires_in * 1000,
})
```

さらに `SKEW_MS = 30_000` の余裕を持たせ、残り30秒を切ったトークンは送らない。到達時に切れている、あるいはクライアントの時計がずれているケースを吸収する。

### 4. 復帰先の判定は `GuestOnly` に集約する

ログインフォームの `onSuccess` で `navigate(from)` を書く方法もあるが、それだと「登録画面から」「401 で飛ばされた後」など経路ごとに同じコードが増える。

`RequireAuth` が `<Navigate to="/login" state={{ from: location }} replace />` で元の場所を渡し、`GuestOnly` が認証済みを検知したら `from ?? "/"` へ送る。**フォーム側は `setSession` を呼ぶだけ**でよく、ログアウトボタンも `logout()` を呼ぶだけで `RequireAuth` が反応する。遷移のトリガーが認証状態の一箇所に寄る。

`replace` を付けないと履歴が `/holdings` → `/login` と積まれ、「戻る」で往復し続けるループができる。

### 5. `initAuth()` で依存を注入する

`client.ts` が `stores/auth.ts` を import し、逆も import すると循環参照になる。Vite の HMR 下では「初期化前の変数にアクセス」という追いにくいエラーで出る。

```ts
// stores/auth.ts
export function initAuth() {
  setTokenProvider(currentToken);
  window.addEventListener(AUTH_EXPIRED, () => useAuthStore.getState().logout());
}
```

`main.tsx` で `createRoot` より**前**に呼ぶ。後に置くと初回描画のリクエストがトークンなしで飛ぶ。

### 6. 4xx はリトライしない

```ts
retry: (failureCount, error) => {
  if (error instanceof ApiError && error.status >= 400 && error.status < 500) return false;
  return failureCount < 2;
}
```

TanStack Query の既定は 3 回リトライ。422 を 3 回投げ直しても結果は同じで、エラー表示が遅れるだけ。**401 の場合は自動ログアウトが 3 回発火する。** `status: 0`（通信エラー）と 5xx はリトライ対象に残す。

### 7. フロント側で 12 文字チェックを先回りしない

条件を 2 箇所に書くと、バックエンドの `validate` を変えたとき片方が古くなる。`hint` で「12文字以上」と予告はするが、判定はサーバーに一本化する。1往復増えるが、「送信できるのに 422 になる」より健全。

### 8. `trace_id` は 500 系のときだけ表示する

`ProblemDetails` には常に `trace_id` が入り、サーバー側も `tracing::error!(%trace_id, ...)` で出力しているため突き合わせが可能。ただし 422 や 409 の画面に UUID を出しても利用者には意味がなく、不安を与えるだけ。`error.status >= 500` の場合のみ小さく添える。

---

## 詰まりどころ

### `GuestOnly.tsx` の中身が `RequireAuth` だった

`App.tsx` に「`GuestOnly` がエクスポートされていない」に加え、`LoginPage` など無関係なモジュールの解決エラーが並んだ。原因は `GuestOnly.tsx` が `RequireAuth.tsx` と同一内容になっていたこと。

**教訓：TS のモジュール解決エラーは連鎖する。** 複数ファイルにエラーが出ても、原因は 1 ファイルであることが多い。エラーの数ではなく、最初に壊れているファイルを探す。チェスアプリで繰り返した「関数定義が消えて呼び出し側だけ残る」と同系統の事故。

### `ProblemDetails` を必須フィールドで固めてしまった

`type` / `status` / `detail` / `trace_id` を必須にしたが、`client.ts` 自身が通信エラー時と非JSON応答時にそれらを持たないオブジェクトを組み立てている。**型と実装が同一ファイル群の中で矛盾していた。**

対応は `title` 以外を optional にする方向。ダミーの `trace_id` を埋める案は採らなかった。**サーバーログと1対1で対応する識別子なので、クライアントが捏造した値が混ざると調査時に嘘をつく。**

### エディタの診断が古い状態を返す

`pages/` 配下のファイル作成後もエラーが残っていた。タスク#13 で `handler/import.rs` の不在をエディタが隠していたのと同じ。

**判定は必ずターミナルで取る。** `npx tsc -b --force` が無言なら通っている。VS Code 側は `TypeScript: Restart TS Server` で追随させる。

### Console への貼り付けが Chrome にブロックされる

トークン改ざんの検証時、「`allow pasting` と入力せよ」という警告が出る。これは自己 XSS 防止の正常な機構で、詐欺で「これをコンソールに貼れ」と誘導される被害を防ぐためのもの。手入力で解除する。

---

## 検証結果

### 401 自動ログアウトの再現手順

`expiresAt` を過去にする方法では**検証にならない**。`currentToken()` が `null` を返してリクエスト自体が飛ばず、401 ハンドラを一度も通らない。署名だけを壊し、クライアント側の期限判定をすり抜けてサーバーの 401 に到達させる。

```js
// Network タブで Preserve log を ON にしてから実行
const k = "asset-log-auth";
const s = JSON.parse(localStorage.getItem(k));
const t = s.state.token;
s.state.token = t.slice(0, -1) + (t.at(-1) === "A" ? "B" : "A");
localStorage.setItem(k, JSON.stringify(s));
location.reload();
```

| 見る場所 | 実測 |
|---|---|
| `me` のステータス | `401 Unauthorized` / `application/problem+json` |
| `me` の回数 | 1 回（`retry` 分岐が機能） |
| Request Headers | `Authorization: Bearer eyJ0eXAi...` あり |
| Response Headers | `Access-Control-Allow-Origin: http://localhost:5173` あり |
| localStorage | `{token: null, expiresAt: null}` |
| URL | `/login` |

**401 にも CORS ヘッダが乗っていることが、ここで実地に裏付けられた。** これが抜けているとブラウザは 401 の本文を読めず「通信エラー」としか表示されず、自動ログアウトも動かない。#17 の完了条件4と #18 の完了条件4は繋がっている。

`WWW-Authenticate: Bearer realm="asset_log"` も確認。`Bearer` なのでブラウザのネイティブ認証ダイアログは出ない（`Basic` だと SPA の画面が壊れる）。

---

## 申し送り

### `DashboardPage` は仮実装

`GET /me` を叩いて `user_id` を表示するだけ。タスク#19 で口座一覧に置き換える。

### 型生成の運用

```json
"scripts": { "gen:api": "npx openapi-typescript ../asset-log/docs/openapi.json -o src/api/schema.d.ts" }
```

openapi-typescript は TS6 未対応のため依存に入れず `npx` で都度実行する方針（#17 で決定）。**生成物 `schema.d.ts` はコミットする。** リポジトリに型がないと `npm ci` 直後にビルドが通らない。バックエンドの API を変更したら `npm run gen:api` を叩く。

### タブ間同期は入れていない

`storage` イベントで別タブのログアウトを反映させる案は見送った。片方でログアウトしても、もう片方は次の API 呼び出しで 401 → 自動ログアウトするため、実害は数秒の表示ずれに留まる。

### 期限切れの先回りタイマーも入れていない

`setTimeout` で期限に合わせてログアウトする案は、スリープ復帰でタイマーがずれるため信頼できない。「API を叩く直前に判定する」現方式のほうが確実。

### フロント用 CI は未整備（#17 からの継続）

`ci.yml` は `paths` フィルタで `asset-log/` 配下のみ発火する。`web/` だけを変更した PR では CI が走らない。#24 で対応する。

### Render の `CORS_ALLOWED_ORIGINS` は未設定（#17 からの継続）

Static Site の URL 確定は #24。設定漏れは「本番でだけ動かない」典型的な事故になる。

---

## 再現コマンド

```bash
# バックエンド起動（コード変更時は --build 必須）
cd ~/workspace/shisan-api
docker compose ps          # api が Up (healthy) か先に確認
docker compose up -d api
```

```bash
# フロント
cd web
npm run gen:api            # openapi.json を更新した場合のみ
npx tsc -b --force
npm run build
npm run dev                # http://localhost:5173
```

### 手動確認シナリオ

| # | 操作 | 期待 |
|---|---|---|
| 1 | `/` を開く（未認証） | `/login` へリダイレクト |
| 2 | `/register` でパスワード5文字 | パスワード欄の下に「12文字以上にしてください」 |
| 3 | メール欄に `abc`（@なし） | メール欄にも同時にエラー（422 の複数 errors） |
| 4 | 正しい値で登録 | `/` へ遷移し `user_id` 表示 |
| 5 | リロード | ログイン状態のまま |
| 6 | 同じメールで再登録 | 上部に 409 のメッセージ |
| 7 | `/login` で誤ったパスワード | 上部に 401 のメッセージ。**画面が飛ばない** |
| 8 | トークン改ざん + リロード（上記スクリプト） | `me` が 401 → 自動ログアウト → `/login` |

7 と 8 が設計判断2の検証点。同じ 401 で挙動が分かれることを確認する。

```js
// 後片付け
localStorage.removeItem("asset-log-auth");
```