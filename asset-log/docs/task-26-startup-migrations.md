# タスク #26: 起動時マイグレーション適用

## 概要

API 起動時に `sqlx::migrate!()` を実行し、DB schema がアプリの期待する状態になる前に HTTP サーバを公開しない構成にする。

現在の `main.rs` にはすでに次の処理が入っている。

```rust
sqlx::migrate!("./migrations")
    .run(&pool)
    .await
    .expect("failed to run migrations");
```

今回の追加作業は、`migrations/` の変更を Cargo の再ビルド対象として明示する `build.rs` の追加。

## 構成

### 起動順序

```text
Config 読込
  ↓
PostgreSQL 接続
  ↓
sqlx::migrate!().run()
  ↓
AppState 構築
  ↓
TcpListener bind
  ↓
HTTP 公開
```

migration に失敗した場合は `expect("failed to run migrations")` で起動自体を失敗させる。

### build.rs

```rust
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
```

`sqlx::migrate!()` は migration をバイナリへ埋め込むため、migration ファイル変更時に Rust source が変わっていなくても再ビルドされる必要がある。

## 検証項目

| # | 確認 | 結果 |
|---|---|---|
| 1 | API 起動前に `sqlx::migrate!()` が実行される | 実装済み |
| 2 | migration failure で API が起動しない | 実装済み |
| 3 | `migrations/` 変更で Cargo rebuild が走る | `build.rs` 追加 |
| 4 | fresh DB で手動 `sqlx migrate run` なしに起動できる | 要ローカル確認 |
| 5 | migration 済み DB へ再起動しても成功する | 要ローカル確認 |

確認手順:

```bash
docker compose down -v
docker compose up -d db

cd asset-log
cargo run
```

別ターミナルで:

```bash
curl http://localhost:8080/health
```

一度停止して再度 `cargo run` し、適用済み migration が原因で失敗しないことも確認する。

## 残課題

migration は forward-only とし、本番で destructive migration を行う場合は別途 expand / contract のデプロイ手順を設計する。
