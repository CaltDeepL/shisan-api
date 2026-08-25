# タスク#14 日次スナップショット（バッチ）

## 概要

`/analytics/asset-history` の評価結果を日次で保存し、表示・集計を高速化する。
正本はあくまで取引履歴からの再計算で、スナップショットはそのキャッシュという位置づけ。

| 項目 | 内容 |
|---|---|
| 実行トリガ | 外部スケジューラから `POST /snapshots/run` を叩く（常駐スケジューラは持たない） |
| 認証 | バッチ専用トークン（`SNAPSHOT_JOB_TOKEN`）。ユーザーJWTとは分離 |
| 保存粒度 | ユーザー × 日 × 口座 × 銘柄（ポジション単位） |
| 読み出し | `asset-history` が範囲を被覆していればキャッシュ、欠けていれば再計算にフォールバック |
| 月次表示 | 月末日の行を読む（別途保存はしない） |
| 失効 | 取引の追加・削除・CSV取込で、影響日以降を削除 |

### 完了条件

1. 同じ範囲を2回実行しても行数・値が変わらない（冪等）
2. `from`/`to` 指定で過去日をバックフィルできる
3. 同一範囲について再計算経路とキャッシュ経路のレスポンスが `source` 以外完全一致する
4. 過去日の取引を追加すると該当日以降が失効し、`source` が `computed` に戻る
5. 保有ゼロの日が「未計算」と誤判定されない
6. 価格未登録銘柄は行が残り、`unpriced_count` に反映される
7. トークン無し・不一致は 401

`tests/snapshots_test.rs` 8ケースで全て担保。`cargo test` 全87件・`clippy`・`fmt --check` パス。

---

## 設計判断

### 1. `snapshot_days` を別テーブルにした

`daily_snapshots` に行が無い日は、次の2つが区別できない。

1. まだバッチが計算していない日
2. 計算済みだが、その日は保有ゼロだった日

キャッシュヒット判定がこれを取り違えると、保有ゼロの日を毎回再計算する（無害だが無意味）か、
**未計算の日を「保有ゼロ」として資産額0で返す**（履歴グラフに谷ができる重大なバグ）ことになる。

そこで「その日を計算済み」を表すマーカー表を分けた。保有ゼロの日も `snapshot_days` には行が入る。
キャッシュ判定は `daily_snapshots` ではなく `snapshot_days` を見る。

### 2. `cost_basis_jpy` は日次レートから復元できない

タスク#11 の決定により、簿価は**約定日レート**でJPY換算した累積値（`Holding.book_value`）。
一方 `fx_rate` 列は評価額換算に使った**当日レート**。
したがって `cost_basis × fx_rate` では簿価を復元できない。

当初は `cost_basis`（資産通貨建て）・`currency`・`fx_rate` も保存する設計にしていたが、
資産通貨建ての簿価はどこにも存在しない（`Holding` が最初からJPY建て）ため列ごと落とした。
残した `price` は「その日どの価格で評価したか」の監査用。
列コメントに復元不可である旨を記載している。

### 3. 失効はユーザー × 日で切る

ポジション単位で消すと「その日の一部のポジションだけ消えた状態」ができ、
`snapshot_days` は残っているのに中身が欠ける、という最悪の組み合わせになる。
キャッシュなので過剰に消して構わない。

```sql
DELETE FROM daily_snapshots WHERE user_id = $1 AND snapshot_on >= $2;
DELETE FROM snapshot_days   WHERE user_id = $1 AND snapshot_on >= $2;
```

いずれも既存の `tx` に相乗りするため、ロールバック時に失効だけ残ることはない。
`snapshot_repo` の全関数が `&mut PgConnection` を受けるのはこのため。

### 4. `upsert_day` は DELETE + INSERT

UPSERT だけにしなかった理由が2つ。

- 前回あって今回消えたポジション（全売却で数量ゼロになった等）が古い値のまま残る
- `CHECK (quantity > 0)` を入れたので数量ゼロの行は保存されず、UPSERTだけでは消す経路が無い

### 5. `spine` は月次でも `from` / `to` を含む

`fetch_price_grid` の `spine` は、`granularity = 'month'` のとき月末に加えて期間の両端も返す。

```
from=2024-01-15, to=2024-03-20, month
→ 2024-01-15, 2024-01-31, 2024-02-29, 2024-03-20
```

キャッシュ判定はこの日付集合と完全一致する必要がある。
Rust側で月末を計算し直すと必ずズレるため、`fetch_target_dates` として同じ SQL を切り出した。
**両者の `WHERE` 句は同期させること。片方だけ直すとキャッシュ判定が壊れる。**

### 6. 評価ロジックを `evaluate_day` に一本化

「正本とキャッシュが一致する」を目標に据えた以上、合算ロジックが2箇所にあると構造的に守れない。
`analytics_service` を3段に分割し、両経路が同じ関数を通るようにした。

```
prepare()          … 取引・価格・為替の読み込み
fold_positions()   … (口座, 銘柄) ごとのタイムライン。group_by 非依存
evaluate_day()     … 1日分・ポジション単位の PositionValue
group_and_series() … 系列への畳み込み。再計算・キャッシュの両経路が共有
```

`PositionTimeline` から `group_key` / `group_label` を外し、分類は
`(account_id, asset_id) → (キー, 表示名)` のマップで後段が解決する形に変えた。
この切り出しは計算内容を一切変えないリファクタリングとして先に独立して実施し、
既存テストが通ることを確認してからスナップショット生成を載せた。

完了条件3が通るのは偶然ではなく、両経路が同じ `evaluate_day` の出力を使うため。

### 7. バッチ認証は `JobAuth` で分離、未設定時は 503

全ユーザー対象の処理なので、ユーザーJWTでの認証は意味を成さない。
`JwtKeys` と同じ流儀で `JobToken: FromRef<S>` を用意し、`JobAuth` 抽出子を新設した。

`SNAPSHOT_JOB_TOKEN` は任意設定にした（バッチを使わないローカル開発を止めないため）。
ただし**未設定なら誰でも通る**という実装は、環境変数の入れ忘れが即座に全開放になるので採らず、
未設定時はエンドポイントを 503 で拒否する。

この結果「トークンが違う（401）」と「機能が無効（503）」が呼び出し側から区別できてしまうが、
運用者が設定漏れに気づけることを優先して許容した。

### 8. `constant_time_eq` を自前実装

通常のバイト比較は不一致位置で早期リターンするため、応答時間の差からトークンを
1バイトずつ推測できる。厳密には `subtle` クレートの `ConstantTimeEq` が正解だが、
依存を増やさない判断で自前実装（`std::hint::black_box` で最適化による早期脱出を抑制）。
長さの一致・不一致は漏れるが、長さは秘密ではないので許容。

### 9. `fx_stale` はキャッシュ経路で常に `false`

スナップショット生成時に為替がキャッシュ由来だったかを記録していない。
記録するなら `snapshot_days` に列が要るため、今回は入れない割り切り。

---

## 実装

| ファイル | 内容 |
|---|---|
| `migrations/0007_snapshots` | `daily_snapshots` / `snapshot_days` の2テーブル |
| `src/config.rs` | `snapshot_job_token: Option<String>`（32バイト以上の検証つき） |
| `src/auth/job.rs` | `JobToken`（保持・定数時間比較） |
| `src/middleware/auth.rs` | `JobAuth` 抽出子 |
| `src/state.rs` | `AppState.job_token` と `FromRef` |
| `src/repository/snapshot_repo.rs` | `upsert_day` / `invalidate_from` / `covered_days` / `find_in_range` |
| `src/service/snapshot_service.rs` | `run` / `run_for_user`、既定は直近7日 |
| `src/handler/snapshots.rs` | `POST /snapshots/run` |
| `src/service/analytics_service.rs` | `prepare` / `fold_positions` / `evaluate_day` / `group_and_series` / `from_snapshots` |
| `src/repository/analytics_repo.rs` | `fetch_target_dates` |
| `src/handler/transactions.rs` | 作成・削除に失効フック |
| `src/service/import_service.rs` | CSV取込に失効フック（取込行の最小 `traded_at` で1回） |
| `tests/snapshots_test.rs` | 8ケース |

### 既定の遡り日数

`from`/`to` 省略時は直近7日を再計算する。過去日の価格・為替は後から登録されうるので、
前日1日だけでは late-arriving data を取りこぼす。

### 為替エラー時の挙動

`asset_history` は `FxError` をそのまま返すが、バッチで同じことをすると
外部API障害時に全ユーザーのスナップショットが1つも作られない。
キャッシュなので、失敗したユーザーはスキップして続行し、`skipped_users` に計上する。

---

## 詰まった点

### `cargo sqlx prepare` は `-- --all-targets` が必要

デフォルトでは lib と bin しか見ないため、`tests/` 内の `sqlx::query!` がキャッシュされない。
`--all-targets` なしで実行すると、**既にキャッシュされていたテスト分を上書きで消す**。

```bash
cargo sqlx prepare -- --all-targets
```

### `DATABASE_URL` があると `.sqlx` より実接続が優先される

キャッシュが揃っていても実DBに繋ぎに行くため、接続が不安定だとビルドが落ちる。
`.cargo/config.toml` で `SQLX_OFFLINE = "true"` を既定にし、
新クエリを書いたときだけ `SQLX_OFFLINE=false cargo sqlx prepare -- --all-targets` を回す運用にした。

### コンテナが落ちているのに気づかず古いプロセスを叩いていた

`docker compose up --build` のログが全レイヤー `CACHED` で、
ビルドコンテキストの転送量も 8.23kB と異常に小さかった。
さらに `docker compose ps` に `api` が出ていなかった。
実際には**ホスト側で起動したままの `cargo run` が 8080 に応答していた**ため、
コードを直しても反映されず、失効フックが動かないように見えていた。

**教訓**: 挙動が変わらないときは、まず `docker compose ps` と `lsof -i :8080` で
「何が応答しているか」を確認する。

### JWT の有効期限（1時間）が手動確認中に切れる

`expires_in: 3600`。長い確認作業では途中で 401 になる。
401 が出たら実装ではなくトークンをまず疑う。

### `/prices` のリクエスト形式

自分の記憶で組み立てて 422 を繰り返した。正解は既存テストに書いてある。

```bash
sed -n '265,285p' tests/assets_test.rs
```

外側に `asset_id`、配列キーは `prices`、各要素は `priced_on` と `price`（**文字列**）。

### 手動 curl より `#[sqlx::test]` の方が確実

トークン期限もサーバー再起動もコンテナ状態も関係なく、毎回クリーンなDBで回る。
今回、手動確認で失効フックの検証に何度も失敗したが、テストに移したら一発で通った。
タスク#13 に続き2回目なので、**次回は動作確認より先にテストを書く**。

---

## 動作確認結果

`2024-01-01`〜`2024-01-31`、1/10に100株（@2400）、1/25に50株（@2550）、
価格は1/15（2500）・1/20（2600）・1/31（2700）。

| ケース | 結果 |
|---|---|
| バッチ実行 | `users: 5, days: 31, rows_upserted: 22, unpriced_rows: 5, skipped_users: 0` |
| 2回目実行 | 数値が完全一致（冪等） |
| `snapshot_days` | 1/1〜1/31 の31行（保有ゼロの1/1〜1/9も含む） |
| `daily_snapshots` | 22行（1/10〜1/31のみ） |
| 1/10〜1/14 | `unpriced = t`、`market_value_jpy` は NULL |
| 1/15〜1/19 | 250000（2500 × 100株） |
| 1/20〜1/30 | 260000（2600がキャリーフォワード） |
| 1/25以降 | 数量150株、簿価367500（加重平均） |
| 1/31 | 405000（2700 × 150株） |
| 認証なし / 誤トークン | 401 |
| トークン未設定時 | 503 |

---

## 残課題

- **422 の content-type が不統一** — axum の `Json` 抽出子が出すデシリアライズエラーは
  ハンドラに入る前に返るため `AppError` の `IntoResponse` を通らず、`text/plain` になる。
  自前のラッパ抽出子で `JsonRejection` を `AppError::UnprocessableEntity` に変換すれば直るが、
  全ハンドラに影響する横断的な変更なので #15 以降で扱う（タスク#13から継続）
- **全ユーザーループのタイムアウト未検証** — 無料枠のリクエストタイムアウトに当たる可能性がある。
  現状は「ユーザーごとに1トランザクション」「`user_id` で分割実行可」の形にしてあるので、
  実測が遅ければ 202 + バックグラウンド処理に寄せる
- **失効が過剰** — CSV取込では `outcome.rows` 全体の最小 `traded_at` を使うため、
  重複スキップされた行の日付も含まれる。キャッシュなので安全側だが、厳密ではない
- **為替の鮮度を記録していない** — スナップショット経路の `fx_stale` は常に `false`
- **`fetch_target_dates` と `fetch_price_grid` の `WHERE` 句が二重管理** — 片方だけ直すと壊れる。
  コメントで注意喚起しているのみ

## 次のタスク

タスク#15（OpenAPI / utoipa）