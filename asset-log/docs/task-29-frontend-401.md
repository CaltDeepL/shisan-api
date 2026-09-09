# タスク #29: フロントエンド 401 の一元処理

## 概要

認証付き API が 401 を返したときの logout / redirect を、各画面ではなく API client と auth store に集約する。

この骨格自体はすでに実装済み。

```text
apiFetch
  ↓ token を付けた request が 401
AUTH_EXPIRED event
  ↓
auth store.logout()
  ↓
RequireAuth が state 変更を購読
  ↓
/login へ redirect
```

今回、ローカルで有効期限切れを検知した場合に stale session が store に残るケースを補強する。

## 既存実装

`apiFetch()` は token を付けた request が 401 の場合だけ `AUTH_EXPIRED` を発火する。login / register は `auth: false` なので、入力ミスによる通常の 401 では自動 logout しない。

## 今回の補強

`currentToken()` で期限切れまたは不完全な session を検出した時点で store から破棄する。

```ts
if (!token || !expiresAt || Date.now() >= expiresAt - SKEW_MS) {
  if (token || expiresAt) {
    useAuthStore.getState().logout();
  }
  return null;
}
```

`initAuth()` でも起動時に `currentToken()` を1回呼び、persist から復元された stale session を掃除する。

## 検証項目

| # | 確認 | 結果 |
|---|---|---|
| 1 | login の認証失敗 401 で global logout event を出さない | 既存実装 |
| 2 | token 付き API の 401 で logout | 既存実装 |
| 3 | logout 後 `RequireAuth` が `/login` へ遷移 | 既存実装 |
| 4 | ローカル期限切れ token を request 前に破棄 | 今回補強 |
| 5 | persist された期限切れ session を起動時に掃除 | 今回補強 |

確認:

```bash
cd web
npm run typecheck
npm run lint
npm run build
```