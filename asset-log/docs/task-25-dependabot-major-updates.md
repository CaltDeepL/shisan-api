# タスク #25: Dependabot major update 対応

## 概要

Dependabot 導入後に残っていた認証系の major update 2件を対応した。

| PR | 更新 | 主な破壊的変更 |
|---|---|---|
| #34 | `argon2 0.5.3` → `0.6.0` | `SaltString` を使う旧 API が廃止され、`hash_password` の呼び出し形が変更 |
| #29 | `jsonwebtoken 9.3.1` → `11.0.0` | crypto backend の明示が必要になり、未指定だと JWT 発行時に panic |

単に `Cargo.toml` のバージョンを上げるだけでは通らず、Argon2 の API 移行と
jsonwebtoken の crypto backend 設定が必要だった。

最初は `rust_crypto` を選択したが、CI に追加した `cargo audit` で
`rsa 0.9.10` の `RUSTSEC-2023-0071` が検出されたため、最終的に
`aws_lc_rs` backend へ変更した。

あわせて、認証系の regression test を既存構成に合わせて
`asset-log/tests/auth_test.rs` に集約した。

## 構成

### Cargo.toml

認証関連の依存は最終的に次の形にした。

```toml
uuid = { version = "1", features = ["v4", "serde"] }

jsonwebtoken = { version = "11", default-features = false, features = ["aws_lc_rs"] }
argon2 = "0.6"

reqwest = { version = "0.12", default-features = false, features = [
    "json",
    "rustls-tls",
    "http2",
] }
````

`argon2 0.6` では salt の生成を内部で行うため、以前使っていた

```toml
rand_core = { version = "0.6", features = ["getrandom"] }
```

の直接依存は削除した。

`jsonwebtoken 11` は crypto backend の明示が必要なため、
最終的に `aws_lc_rs` を選択した。

`use_pem` は使用していないため `default-features = false` にしている。

### Argon2

0.5 系ではアプリ側で salt を生成していた。

```rust
let salt = SaltString::generate(&mut OsRng);

Argon2::default()
    .hash_password(plain.as_bytes(), &salt)
```

0.6 では `hash_password` が salt を内部生成するため、次の形に変更した。

```rust
Argon2::default()
    .hash_password(plain.as_bytes())
    .map(|h| h.to_string())
```

最終的な `password.rs` では `rand_core::OsRng` と `SaltString` を使用しない。

```rust
use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use std::sync::LazyLock;

/// ユーザー不在時に検証を空回しするためのダミー。
/// 起動時に1回だけ計算する。
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("dummy-password-for-timing-equalization")
        .expect("ダミーハッシュの生成に失敗")
});

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(plain.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}
```

既存の `verify_dummy` / `warmup` はそのまま維持している。

### jsonwebtoken

`jsonwebtoken 11` では crypto backend を明示しないと、
compile 自体は通っても JWT 発行時に panic する。

Dependabot PR #29 では integration test で JWT を発行した際に、
provider を自動決定できないという runtime error が発生した。

対策として、アプリ起動コードから `CryptoProvider::install_default()` を呼ぶのではなく、
依存定義側で backend を一意に決める方針にした。

当初は次の設定を採用した。

```toml
jsonwebtoken = { version = "11", default-features = false, features = ["rust_crypto"] }
```

この設定で JWT の issue / verify は正常に動作したが、
CI の `cargo audit` で `rust_crypto` 側から入る `rsa 0.9.10` に
`RUSTSEC-2023-0071` が検出された。

修正版が存在しない advisory だったため、警告を ignore して CI を通すのではなく、
backend を `aws_lc_rs` へ変更した。

```toml
jsonwebtoken = { version = "11", default-features = false, features = ["aws_lc_rs"] }
```

HS256 を使う既存の `JwtKeys` の公開 API と `jwt.rs` のロジックは変更していない。

### テスト配置

このリポジトリでは integration test を `asset-log/tests/` に集約しているため、
今回も `src/auth/*.rs` に `#[cfg(test)] mod tests` は置かず、
`tests/auth_test.rs` を追加した。

```text
asset-log/
├── src/
│   └── auth/
│       ├── password.rs
│       └── jwt.rs
└── tests/
    └── auth_test.rs
```

追加したテストは4件。

| テスト                                   | 確認内容                                    |
| ------------------------------------- | --------------------------------------- |
| `password_hash_and_verify_round_trip` | 新規 hash が正常に検証でき、誤パスワードを拒否する            |
| `password_verifies_legacy_phc_hash`   | 旧 Argon2 PHC 文字列を 0.6 でも検証できる           |
| `password_rejects_malformed_hash`     | 不正な PHC 文字列を拒否する                        |
| `jwt_issue_and_verify_round_trip`     | jsonwebtoken 11 で JWT 発行・検証が panic せず通る |

パスワードハッシュは DB に永続化されるため、
新規 hash の生成だけでなく既存 PHC 文字列の互換性も regression test に固定した。

## つまずいた点

### Cargo.toml から argon2 / jsonwebtoken を消した状態で cargo update を実行した

修正途中で `Cargo.toml` から `argon2` / `jsonwebtoken` の依存定義が抜けた状態になった。

その状態で

```bash
cargo update -p argon2 --precise 0.6.0
```

を実行したところ、Cargo は更新ではなく
「現在の dependency graph では不要」と判断し、旧 package を `Cargo.lock` から削除した。

実際の出力では次のようになった。

```text
Removing argon2 v0.5.3
Removing jsonwebtoken v9.3.1
```

続けて同じコマンドを実行すると、

```text
error: package ID specification `argon2` did not match any packages
```

になった。

`cargo update -p` は dependency を追加するコマンドではなく、
すでに dependency graph に存在する package の lockfile 解決を更新するコマンド。

先に `Cargo.toml` へ直接依存を戻し、その後 Cargo に dependency resolution を行わせた。

今後 major update を手動対応するときは、
`cargo update -p` の前に `Cargo.toml` の直接依存が残っているかを確認する。

### argon2 0.5 / 0.6 のコードを混在させた

最初の修正では import の一部を 0.6 向けに変更した一方で、
旧 API のコードを残してしまった。

```rust
use rand_core::OsRng;

let salt = SaltString::generate(&mut OsRng);
```

一方 `Cargo.toml` から `rand_core` は削除していたため、
次の compile error になった。

```text
error[E0432]: unresolved import `rand_core`
error[E0433]: cannot find type `SaltString` in this scope
```

原因は、Argon2 0.6 への移行を import だけの変更として扱い、
旧 salt 生成処理を消し切れていなかったこと。

修正後は `OsRng` / `SaltString` を完全に削除し、0.6 の API に統一した。

```rust
Argon2::default()
    .hash_password(plain.as_bytes())
```

major update では compile error が出た行だけを逐次直すのではなく、
旧 API に依存している一連の処理をまとめて置き換える必要がある。

### jsonwebtoken 11 は compile が通っても runtime で落ちる

PR #29 は compile / Clippy までは通ったが、
integration test で JWT を発行した時点で runtime error になった。

原因は jsonwebtoken 11 で crypto backend が暗黙選択されなくなったこと。

この種の変更は `cargo check` だけでは検出できないため、
JWT の issue / verify を実際に通す regression test を追加した。

依存更新では compile green だけでなく、
対象ライブラリの主要 runtime path までテストする必要がある。

### rust_crypto では cargo audit が失敗した

jsonwebtoken 11 の provider 問題を解消するため、
当初は `rust_crypto` backend を指定した。

JWT の実行テスト自体は通ったが、CI の Security audit で次の vulnerability が検出された。

```text
Crate:   rsa
Version: 0.9.10
Title:   Marvin Attack: potential key recovery through timing sidechannels
ID:      RUSTSEC-2023-0071
Severity: 5.9 (medium)
Solution: No fixed upgrade is available
```

現在のアプリは HS256 を利用しており RSA JWT を使用していないが、
`rust_crypto` feature により `rsa` crate が dependency graph に含まれていた。

修正版のない vulnerability を allowlist して残すより、
利用可能な別 backend に切り替える方針を採用した。

```toml
jsonwebtoken = { version = "11", default-features = false, features = ["aws_lc_rs"] }
```

変更後に CI を再実行し、Security audit を含めて green になった。

### cargo audit の unmaintained warning は vulnerability と分けて扱う

Security audit では `paste 1.0.15` に対する unmaintained warning も表示された。

```text
Crate:   paste
Version: 1.0.15
Warning: unmaintained
Title:   paste - no longer maintained
ID:      RUSTSEC-2024-0436
```

これは CI で allowed warning として扱われており、
今回の exit code 1 の原因ではなかった。

今回の blocking issue は `rsa 0.9.10 / RUSTSEC-2023-0071` の vulnerability だったため、
warning と vulnerability を分けて診断した。

## 副次的な修正

### rand_core の直接依存を削除

argon2 0.5 時代は salt 生成のため `rand_core::OsRng` を直接使っていたが、
0.6 では不要になった。

そのため `Cargo.toml` から直接依存を削除した。

結果として、アプリコード側で乱数生成の具体実装を持たず、
Argon2 crate の API に任せる形になった。

### 認証テストを tests/auth_test.rs に集約

当初は `password.rs` / `jwt.rs` に `#[cfg(test)] mod tests` を置いていたが、
既存リポジトリでは `accounts_test.rs`、`analytics_test.rs`、
`openapi_test.rs` などを `tests/` にまとめている。

今回のテストは private helper を直接触る必要がなく、
公開 API だけで検証できるため、既存方針に合わせて `tests/auth_test.rs` に移した。

`src` 配下は実装、`tests` 配下は外部から見た regression test、
という境界が明確になった。

### 旧 PHC 互換性を固定

依存更新後も既存ユーザーの保存済み password hash が使えることを確認するため、
Argon2id v19 の固定 PHC fixture を追加した。

新規 hash の round trip だけでは既存 DB の認証データ互換性までは保証できないため、
major update の regression として残している。

### Security audit を CI に追加

依存の major update を compile / test だけで判断しないよう、
CI に `cargo audit` を追加した。

```yaml
- name: Security audit
  run: |
    command -v cargo-audit >/dev/null || cargo install cargo-audit --locked
    cargo audit
```

今回 `rust_crypto` から入った `rsa` vulnerability を merge 前に検出できたため、
依存更新に対する防波堤として実際に機能した。

## 検証項目

| #  | 確認                                          | 結果                            |
| -- | ------------------------------------------- | ----------------------------- |
| 1  | `argon2 v0.6.0` が compile される               | OK                            |
| 2  | `jsonwebtoken v11.0.0` が compile される        | OK                            |
| 3  | jsonwebtoken の backend が `aws_lc_rs` に固定される | OK                            |
| 4  | 新規 password hash → verify が通る               | OK                            |
| 5  | 誤パスワードを拒否する                                 | OK                            |
| 6  | 旧 Argon2 PHC を 0.6 で検証できる                   | OK                            |
| 7  | malformed PHC を拒否する                         | OK                            |
| 8  | JWT issue → verify が panic せず通る             | OK                            |
| 9  | `cargo fmt --all -- --check`                | OK                            |
| 10 | `cargo clippy --all-targets -- -D warnings` | OK                            |
| 11 | `cargo test --all-targets`                  | OK（107 tests）                 |
| 12 | `cargo audit`                               | OK（blocking vulnerability なし） |
| 13 | PR #38 の CI                                 | OK                            |
| 14 | main merge 後の CI                            | OK                            |

ローカルでは認証テスト4件を含む全107テストが通った。

```text
test password_rejects_malformed_hash ... ok
test jwt_issue_and_verify_round_trip ... ok
test password_verifies_legacy_phc_hash ... ok
test password_hash_and_verify_round_trip ... ok
```

最終的に PR #38 を main へ merge し、merge 後の main CI まで green を確認した。

## 残課題

| 項目                     | 内容                                                              |
| ---------------------- | --------------------------------------------------------------- |
| JWT expiry             | 有効期限切れ token の拒否テストを追加したい                                       |
| JWT tamper             | payload 改ざん token の拒否を regression test にしたい                     |
| wrong signing secret   | 別 secret で署名された token を拒否することを固定したい                             |
| パスワード要件                | Unicode 文字数、byte 上限、common-password denylist を追加したい             |
| 起動時 migration          | `sqlx::migrate!()` を起動時に実行し、ローカル / 本番だけ migration 漏れで壊れる状態を防ぎたい |
| OpenAPI error contract | 全 4xx / 5xx が `ProblemDetails` を返すか自動検証したい                      |
| フロント 401               | API wrapper で 401 を一元検知し、認証切れ時の logout / redirect を統一したい        |

今回の `auth_test.rs` は Dependabot major update の破壊的変更を防ぐための最小 regression。

認証仕様そのものの強化は別タスクに分離する。

```
```
