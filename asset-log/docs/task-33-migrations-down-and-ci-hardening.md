# タスク #33: マイグレーションの可逆化と CI / Dependabot 設定の見直し

## 概要

全体の見直しで見つかった次の4点をまとめて修正した。いずれもアプリケーションコードと API 契約は変更していない。

| # | 対象 | 問題 | 対応 |
|---|---|---|---|
| 1 | `migrations/0005`, `0006` | この2本だけ一方向（`.sql`）で、`.down.sql` が無かった | `.up.sql` / `.down.sql` の可逆形式へそろえる |
| 2 | `.github/dependabot.yml` | npm の `types` グループに `update-types` が無く、`@types/*` の major もまとめて来る | `types` を minor / patch に限定する |
| 3 | `.github/workflows/snapshot.yml` | `${{ inputs.* }}` / `${{ secrets.* }}` をスクリプトへ直接埋め込んでいた | `env:` 経由で渡し、日付形式を検証する |
| 4 | `.github/workflows/deploy.yml` | `workflow_run` の起点イベントを見ていなかった | `workflow_run.event == 'push'` を条件に加える |

## 1. マイグレーションの可逆化

### 変更内容

```text
0005_fx_rates.sql                       → 0005_fx_rates.up.sql（内容は変更なし）
                                          0005_fx_rates.down.sql（新規）
0006_add_transactions_external_id.sql   → 0006_add_transactions_external_id.up.sql（内容は変更なし）
                                          0006_add_transactions_external_id.down.sql（新規）
```

```sql
-- 0005_fx_rates.down.sql
DROP TABLE IF EXISTS fx_rates;

-- 0006_add_transactions_external_id.down.sql
DROP INDEX IF EXISTS transactions_user_external_id_key;
ALTER TABLE transactions DROP COLUMN IF EXISTS external_id;
```

down は既存の 0004 と同じく `IF EXISTS` を付けた。

### 適用済みの DB（本番）に影響しない理由

sqlx は `_sqlx_migrations` に **version と SQL 本文のチェックサム**を記録し、起動時に照合する。ファイル名の `.sql` → `.up.sql` は形式の違いであって、version・description・本文は変わらないため、チェックサムも一致する。

- description はどちらも `fx rates` / `add transactions external id`（拡張子は含まれない）
- `.up.sql` の本文は rename 前の `.sql` と1バイトも変えていない

本文を少しでも変えると、起動時の `sqlx::migrate!()` が `migration N was previously applied but has been modified` で失敗し、**API が起動しなくなる**。適用済みのマイグレーションは本文を触らないこと。

### 検証

一時的な Postgres 17 コンテナで、本番と同じ状態を再現して確認した。

| # | 手順 | 結果 |
|---|---|---|
| 1 | 変更**前**のファイル構成で `sqlx migrate run`（本番相当の DB を作る） | 7本適用 |
| 2 | 変更**後**のファイル構成で同じ DB に `sqlx migrate run` | エラーなし・適用0件（チェックサム一致） |
| 3 | `sqlx migrate info` | 7本すべて `installed` |
| 4 | `sqlx migrate revert --target-version 0` | 7 → 1 の順にすべて revert 成功。テーブル・ENUM が残らない |
| 5 | 再度 `sqlx migrate run` し、`pg_dump -s` を手順1の直後と比較 | 差分なし（pg_dump が毎回付ける `\restrict` トークンのみ） |
| 6 | `cargo test --all-targets`（`#[sqlx::test]` が埋め込みマイグレーションを使う） | 118件 pass |
| 7 | `cargo sqlx prepare --check -- --all-targets` | 成功 |

```bash
# 手順2〜5の再現
cd asset-log
sqlx migrate run --source migrations
sqlx migrate info --source migrations
pg_dump -s -T _sqlx_migrations "$DATABASE_URL" > before.sql
sqlx migrate revert --source migrations --target-version 0
sqlx migrate run --source migrations
pg_dump -s -T _sqlx_migrations "$DATABASE_URL" > after.sql
diff before.sql after.sql
```

### 運用ルール

- 新しいマイグレーションは `sqlx migrate add -r <name>` で **up / down の組**として作る
- down を書いたら、`revert` → `run` を往復してスキーマが元に戻ることを確認する
- 本番で `revert` を使うのは最後の手段。データを伴うテーブルの down はデータも消える（`fx_rates` はキャッシュなので再取得できるが、`transactions.external_id` は消えると CSV の重複判定が効かなくなる）

## 2. Dependabot: `@types/*` の major を個別 PR にする

方針（ファイル冒頭のコメント）は「major は破壊的変更を含むので個別の PR にする」だが、npm の `types` グループは `patterns` だけで `update-types` が無かったため、`@types/*` は major も含めて1本にまとめられていた。

`@types/node` の major は Node 本体の major に対応し、`web/.node-version` と合わせて判断が要る。ほかの依存と同じく minor / patch だけをまとめるようにした。

```yaml
types:
  patterns:
    - "@types/*"
  update-types:
    - "minor"
    - "patch"
```

cargo の `utoipa` グループは、意図的に major も含めてまとめている（task #25 / PR #65）。こちらは例外としてファイル冒頭に理由を書いてある。

## 3. snapshot.yml: 式展開をスクリプトに直接埋め込まない

`${{ }}` はシェルが実行される**前に**文字列として置き換えられる。手動実行（`workflow_dispatch`）の `from` / `to` に `"; コマンド; "` のような値を入れると、スクリプトの一部として実行されてしまう。

手動実行できるのは書き込み権限のある人だけなので危険度は低いが、GitHub の推奨どおり次のようにした。

- `inputs` と `secrets` は `env:` 経由で渡し、シェル変数（`"$INPUT_FROM"` など）として参照する
- 値は JSON の本文に埋め込むので、`YYYY-MM-DD` 形式以外はエラーで止める

ローカルの bash で、空・正しい日付・`"; echo pwned; "`・`10/01` を入れ、前の2つが通り、後の2つが弾かれることを確認した。

## 4. deploy.yml: push 起点の CI だけをデプロイする

`workflow_run` の `branches: [main]` は、CI を起動した **head ブランチ名**での絞り込み。PR の head ブランチがたまたま `main` という名前（フォークからの PR など）でも一致する。

Deploy Hook がデプロイするのは常に Render 側の `main` なので実害はほぼ無いが、「main への push で CI が通ったらデプロイする」という意図をはっきりさせるため、各ジョブの条件に `github.event.workflow_run.event == 'push'` を加えた。手動実行（`workflow_dispatch`）の条件は変更していない。

## 検証まとめ

- [x] 適用済み DB に対して変更後の構成で `migrate run` が成功する（チェックサム一致）
- [x] 全 revert → 再適用でスキーマが完全に一致する
- [x] `cargo fmt --check` / `cargo clippy -D warnings` / `cargo test --all-targets`（118件）/ `cargo sqlx prepare --check` 成功
- [x] 変更した3つの YAML が Ruby の YAML parser で読める
- [x] snapshot.yml の日付検証をローカルの bash で確認

## 残課題

作業中の `cargo audit` で、今回の変更とは無関係の新しい advisory が検出された。`main` でも同じく失敗する。task #32 の方針（無関係な依存更新を同じ変更へ混ぜない）に従い、別の対応とする。

| Crate | Version | ID | 修正版 | 経路 |
|---|---|---|---|---|
| rustls | 0.23.45 | RUSTSEC-2026-0346 / 0347 / 0348 / 0349 | 0.23.46 | reqwest / sqlx 経由。`Cargo.lock` の更新だけで解消できる |
| zip | 8.6.0 | RUSTSEC-2026-0367 / 0368 / 0369 | 9.0.0 | `utoipa-swagger-ui 10.0.1` の build-dependency。最新版でも 8.x を使っており、上流の更新待ち |
