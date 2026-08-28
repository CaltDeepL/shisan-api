# タスク#16 デプロイ・GitHub Actions

Render + Neon に本番環境を構築し、CI が通ったときだけ自動デプロイされる状態にした。日次スナップショットは GitHub Actions の schedule から HTTP で起動する。

公開URL: https://shisan-api.onrender.com

## 採用した構成

| 項目 | 選択 | 理由 |
|---|---|---|
| アプリ | Render Web Service（Docker、Singapore、無料枠） | 既存の Dockerfile がそのまま使える。カード登録不要 |
| DB | Neon（Postgres 17、Singapore） | Render の無料 Postgres は期限付きで削除されるため、取引履歴の置き場所にできない |
| デプロイ起動 | Render の auto-deploy を切り、Actions から Deploy Hook | auto-deploy は CI の成否と無関係に走ってしまう |
| バッチ起動 | Actions の `schedule`（JST 07:00） | 常駐スケジューラを持たない #14 の設計に合わせた |
| マイグレーション | アプリ起動時に `sqlx::migrate!` で自動適用 | 単一インスタンスのため競合しない。手順を1つ減らせる |

Fly.io・Koyeb・Cloud Run・GCP e2-micro も比較したが、Fly.io は無料枠が廃止済み、Koyeb はリージョンが欧米限定、Cloud Run は WIF の設定コストが高い。「運用サイクルを先に一周させる」ことを優先し、chess-app で経験のある Render を選んだ。

## ワークフロー構成

.github/workflows/
ci.yml push/PR → fmt / clippy / test（Postgres service container）
deploy.yml workflow_run: CI 完了かつ success かつ main → Deploy Hook を叩く
snapshot.yml schedule (UTC 22:00) / workflow_dispatch → ウォームアップ → POST /snapshots/run


`deploy.yml` は `if: github.event.workflow_run.conclusion == 'success'` が要。これが無いと CI 失敗時にもデプロイされ、Render の auto-deploy を切った意味が消える。

`snapshot.yml` は無料枠のスピンダウン（15分無通信でスリープ、復帰に約50秒）に対応するため、`/health` が200を返すまで最大10回（15秒間隔）叩いてから本命を呼ぶ。

## 本番向けに調整した箇所

### PgPool

Neon は5分無通信で compute が停止し、この挙動は無効化できない。

```rust
PgPoolOptions::new()
    .max_connections(5)
    .min_connections(0)          // アイドル接続を維持しない（CU時間の無駄）
    .acquire_timeout(30s)
    .idle_timeout(120s)          // Neon の5分停止より先にプール側から畳む
    .max_lifetime(1800s)
    .test_before_acquire(true)   // コールドスタート直後の500を潰す
```

### sqlx の TLS

`Cargo.toml` の sqlx に `tls-rustls-ring` を追加。ローカルは平文接続だったため、Neon の `sslmode=require` で初めて露見した。

## 詰まった点

| 事象 | 原因と対処 |
|---|---|
| CI の clippy が `dead_code` で落ちる | 統合テストは `tests/` 直下の各ファイルが独立クレートで、`common/mod.rs` は毎回丸ごと取り込まれる。あるバイナリから見れば未使用の関数が必ず出る。ファイル先頭に `#![allow(dead_code)]` |
| 起動時に `SQLx was built without TLS support` | 上記の sqlx TLS フィーチャー欠落 |
| Manual Deploy が古いコミットで動く | 修正が `main` にマージされていなかった。Render が見るのは `main` |
| `deploy.yml` が初回だけ発火しない | `workflow_run` はデフォルトブランチにワークフローが載って以降のイベントにしか反応しない |
| ワークフローだけ直しても CI が走らない | `ci.yml` の `paths: asset-log/**` フィルタでスキップされ、Deploy も連鎖しない |
| `/snapshots/run` が400 | ハンドラは `body: Option<Json<RunRequest>>` でボディ任意。`Content-Type: application/json` だけ付けてボディを送らないとパース失敗になる。ヘッダごと外す |
| zsh で `read -s -p` が動かない | `read: -p: no coprocess`。zsh は `read -rs "VAR?prompt: "` |

## 決めごと

- Secrets は GitHub 側に `RENDER_DEPLOY_HOOK_URL` / `API_BASE_URL` / `SNAPSHOT_JOB_TOKEN`、Render 側に `DATABASE_URL` / `JWT_SECRET` / `SNAPSHOT_JOB_TOKEN` / `RUST_LOG`。`PORT` は Render が注入するので設定しない
- `DATABASE_URL` は Neon の pooled エンドポイント。`sqlx migrate run` を手で流すときだけ direct を使う
- CORS は未実装。フロントエンド着手時に `FRONTEND_ORIGIN` ごと追加する

## 残課題

| 項目 | 内容 |
|---|---|
| reqwest の依存整理 | `default-features = false` にして `hyper-tls` を削る。`blocking` も不要 |
| 日次実行の範囲 | ボディ無しだと毎回7日分を再計算する。冪等だが `from`/`to` で1日分に絞る余地 |
| マイグレーション実行位置 | 複数インスタンスにする段階で Render の Pre-Deploy Command へ移す |
| `cargo sqlx prepare --check` | `.sqlx` の陳腐化を CI で検知していない。sqlx-cli のインストールに時間がかかるため別ジョブが妥当 |
| `actions/checkout@v4` | Node.js 20 非推奨の警告。v5 に上げる |