# タスク #28: JWT / Argon2 regression test 拡張

## 概要

Dependabot major update 時に追加した最小4テストを、認証境界として必要な regression test まで拡張する。

sleep を使う期限切れテストは不安定になるため採用しない。

## Argon2

| テスト | 内容 |
|---|---|
| round trip | hash → verify |
| random salt | 同じ password を2回 hash して異なる PHC になる |
| legacy PHC | 保存済み PHC を検証 |
| malformed PHC | 不正文字列を false で拒否 |

## JWT

| テスト | 内容 |
|---|---|
| issue / verify | HS256 の正常系 |
| expiry | すでに期限切れの claims を直接 encode して拒否 |
| tamper | signature を1文字変更して拒否 |
| wrong secret | 別 secret で署名された token を拒否 |
| algorithm pinning | HS384 token を HS256 verifier が拒否 |

### expiry で sleep しない

```text
now
 ├─ iat = now - 120
 └─ exp = now - 60
```

期限切れ claims をテスト内で直接 encode するため、実時間待機や CI のタイミング差に依存しない。

## パスワード policy

タスク #27 の登録 API validation も同じ `tests/auth_test.rs` に置く。認証関連テストを `src` 内へ分散させず、既存方針どおり `asset-log/tests/` に集約する。

## 検証

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo audit
```