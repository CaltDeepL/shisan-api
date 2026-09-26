# タスク #32: rustls 脆弱性 RUSTSEC-2026-0285 対応

## 概要

GitHub Actions の `Backend (Rust)` ジョブ内 `Security audit` で `cargo audit` が失敗し、間接依存の `rustls 0.23.44` に RUSTSEC-2026-0285 が検出されたため、`rustls 0.23.45` へ更新した。

修正は `Cargo.lock` の依存解決結果のみで、アプリケーションコードや API 契約の変更はない。

## 背景

Dependabot PR の CI 実行時に、次の advisory が検出された。

```text
Crate:    rustls
Version:  0.23.44
Title:    TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries
ID:       RUSTSEC-2026-0285
Severity: 5.3 (medium)
Solution: Upgrade to >=0.23.45
```

同時に `paste 1.0.15` の RUSTSEC-2024-0436（unmaintained）も報告されたが、CI では allowed warning として扱っており、今回の失敗原因ではない。

## 対応

`asset-log` で `rustls` の lockfile 解決を 0.23.45 へ更新した。

```bash
cd asset-log
cargo update -p rustls --precise 0.23.45
```

変更対象:

```text
asset-log/Cargo.lock
```

更新後:

```text
rustls 0.23.45
```

`Cargo.toml` の直接依存バージョンやアプリケーションコードは変更していない。既存の依存制約内で解決可能な patch update として処理した。

## Git / PR

- 修正コミット: `df14fcf`（`fix:rustls 0.23.44 の脆弱性更新`）
- PR: [#54 fix:rustls 0.23.44 の脆弱性更新](https://github.com/CaltDeepL/shisan-api/pull/54)
- マージコミット: `fc9a26a`（`Merge pull request #54 from CaltDeepL/fix/rustls-rustsec-2026-0285`）

`main` は直接 push を禁止しているため、修正ブランチ `fix/rustls-rustsec-2026-0285` から PR を経由して取り込んだ。

## 検証

```bash
cd asset-log

cargo tree -i rustls
cargo audit
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

確認結果:

- `cargo tree -i rustls --locked --offline` で `rustls v0.23.45` を確認
- `cargo audit --no-fetch` は終了コード 0 で、RUSTSEC-2026-0285 が検出されないことを確認
- PR #54 の CI run #105 が成功
- `Backend (Rust)` で format check、Clippy、全ターゲットのテスト、`cargo audit` が成功
- `paste 1.0.15` の unmaintained advisory は既存の allowed warning として1件残る

## 完了条件

- [x] `rustls` を 0.23.45 以上へ更新
- [x] 変更を `Cargo.lock` に限定
- [x] protected `main` へ直接 push せず PR を経由
- [x] PR #54 を `main` にマージ
- [x] README のメンテナンス実績へ本対応を追記

## 残課題

`paste 1.0.15` の RUSTSEC-2024-0436 は「脆弱性」ではなく unmaintained advisory として現在の CI では許容している。直接依存ではないため、親依存の更新で除去可能かを別途確認する。

CI 復旧のためだけに allowlist を拡張したり、無関係な依存更新を同じ変更へ混ぜない。
