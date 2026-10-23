# CLAUDE.md

AI エージェント（Claude Code・Cursor など）向けの作業ガイド。人向けの詳細は README.md を正とする。

## プロジェクト

証券口座・銀行口座をまたいで資産と取引を管理する Web アプリ。本番は Render（API・Static Site）+ Neon（PostgreSQL）で稼働中。

- バックエンド: `asset-log/`（Rust / Axum / SQLx / utoipa）。層は `handler` → `service` → `repository`、外部 API は `provider/` の trait
- フロントエンド: `web/`（React / TypeScript / Vite / TanStack Query / Recharts / Tailwind CSS）
- バージョン固定: Rust は `asset-log/rust-toolchain.toml`、Node は `web/.node-version`

## よく使うコマンド

```bash
# 起動（リポジトリ直下）
docker compose up --build -d && curl -fsS http://localhost:8080/health

# バックエンド（asset-log/ で実行）
cd asset-log
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo sqlx prepare --check -- --all-targets   # SQL を変えたら `cargo sqlx prepare -- --all-targets` で .sqlx/ を更新

# フロントエンド（web/ で実行）
cd web
npm ci
npm run gen:api            # asset-log/docs/openapi.json から src/api/schema.d.ts を生成
npm run typecheck
npm run lint
npm run build
```

ローカルのポート: API `8080`、PostgreSQL `5432`、フロント `5173`。

## 守ること

- エラー応答は RFC 9457（Problem Details）。DB の制約違反は適切な HTTP ステータスに変換し、内部情報は返さない
- 金額・数量は `Decimal`（DB は NUMERIC）。浮動小数点を使わない
- 評価・集計のロジックは `domain/` の純粋関数に置き、ユニットテストする
- API を変えたら OpenAPI（utoipa）を更新し、`docs/openapi.json` と `web/src/api/schema.d.ts` を再生成してコミットする（CI が差分を検知する）
- 外部 API は trait で抽象化し、テストでは wiremock でスタブ化する
- HTTP ステータスだけでなく、DB の状態までテストで確認する
- utoipa 系のクレートはまとめて上げる（別々に上げるとビルドが壊れる）

## 作業の進め方

- タスクは GitHub Issue（ラベル `task`、テンプレートは [CaltDeepL/.github](https://github.com/CaltDeepL/.github)）で管理し、作業メモもその Issue に書く（2026-10-24 以降）
- 過去の作業メモ `asset-log/docs/task-*.md` は記録として残す。新しく追加しない
- ブランチ → PR → CI → squash マージ。`main` へ直接 push しない
- コミットは Conventional Commits。PR 本文に `Closes #NN` を書く

## やってはいけないこと

- `.env` や秘密情報（JWT_SECRET・SNAPSHOT_JOB_TOKEN・Deploy Hook URL）をコミット・出力しない
- `main` の Ruleset を変更しない、force push しない
- 確認なしに本番へのデプロイ、Issue・PR の作成、GitHub の設定変更をしない
