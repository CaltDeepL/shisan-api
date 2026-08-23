# タスク#12：資産配分（GET /analytics/allocation）

| 項目 | 内容 |
|---|---|
| ゴール | 保有資産の構成比を分類軸ごとに返すエンドポイントを追加する |
| 完了条件 | 各項目の比率の合計がちょうど 100% になる |
| 結果 | 達成。ユニットテスト7件・統合テスト6件を追加し、全57→70件が green |
| 追加/変更ファイル | `src/service/allocation_service.rs`（新規）、`src/service/analytics_service.rs`、`src/repository/analytics_repo.rs`、`src/handler/analytics.rs`、`src/service/mod.rs`、`src/lib.rs`、`tests/analytics_test.rs`、`.sqlx` |

---

## 1. 実装したもの

### 1.1 エンドポイント

```
GET /analytics/allocation?as_of=2026-01-10&group_by=asset_class
```

| パラメータ | 既定値 | 内容 |
|---|---|---|
| `as_of` | 当日（JST） | 評価基準日。未来日は 422 |
| `group_by` | `asset_class` | `asset_class` / `account_type` / `account` / `asset` の4軸。`none` は 422 |

レスポンス例。

```json
{
  "as_of": "2026-01-10",
  "base_currency": "JPY",
  "group_by": "asset_class",
  "scope": "securities_only",
  "fx_stale": false,
  "total_value_jpy": "3000000",
  "unpriced_asset_count": 1,
  "items": [
    { "key": "equity", "label": "株式", "value_jpy": "1000000", "ratio": "33.34" },
    { "key": "etf",    "label": "ETF",  "value_jpy": "1000000", "ratio": "33.33" },
    { "key": "mutual_fund", "label": "投資信託", "value_jpy": "1000000", "ratio": "33.33" }
  ]
}
```

`items` は `value_jpy` の降順。

### 1.2 付随する変更（#11 への影響）

| 変更 | 内容 | 影響 |
|---|---|---|
| `HistoryTrade` にフィールド追加 | `account_name` / `symbol` / `asset_name` | 内部型。外部契約への影響なし |
| `HistorySeries` に `label` 追加 | 分類軸に応じた表示名 | **`GET /analytics/asset-history` のレスポンスにも `label` が乗る**。純粋な追加なので既存クライアントは壊れない |
| `GroupBy` に `Account` / `Asset` 追加 | 口座別・銘柄別 | asset-history 側でも使えるようになった |

---

## 2. 設計判断とその根拠

### 2.1 独自クエリを作らず、#11 の `asset_history` に相乗りする

当初は `analytics_repo` に単一時点用のクエリを新設する予定だったが、既存コードを読んだ結果**「`from = to = as_of` の1日だけの時系列」として `asset_history` を呼べば済む**ことが分かったため、方針を変更した。

```rust
let history = analytics_service::asset_history(
    db, fx, user_id, as_of, as_of, Granularity::Day, group_by,
).await?;
```

採用理由は**数値の不一致を構造的に防ぐ**こと。別クエリを書くと、`unpriced` の扱い・取得日レートの適用・`price_unit` の除算のいずれかで、いつか必ず asset-history とズレる。「折れ線グラフの合計と円グラフの合計が合わない」は実運用で最も気づきにくいバグであり、経路を1本にしておけば原理的に起こらない。

この判断を守るため、統合テスト `allocation_matches_history_total` で両エンドポイントを実際に叩いて合計の一致を検証している。将来 allocation を独自クエリに最適化したくなったとき、このテストがズレを即座に検出する。

コストは1点。1日分しか要らないのに `fetch_price_grid` が `generate_series` を回すが、範囲が1日なので実質的な差はない。

### 2.2 比率は最大剰余法で配分する

単純な四捨五入では合計が 99.99 や 100.01 に振れ、完了条件を満たせない。以下の手順を採った。

1. 生比率を `value * 10000 / total` で Decimal のまま計算（0.01 を1単位とする整数化）
2. `floor()` で切り捨て（この時点で合計は 10000 以下）
3. `10000 - 切り捨て後の合計` を求める
4. 切り捨てた端数が大きい順に 1 単位ずつ配る

不足分が項目数を超えないことは証明できる。各項目が切り捨てで失うのは 1 単位未満なので、総損失は「項目数 × 1 未満」に収まるため。

**タイブレークを2段構えにした**のが実装上の要点。3等分のように端数が完全同値になるケースは実際に起きるので、そこで `HashMap` の反復順が漏れ出すとテストが実行ごとに落ちる。

```rust
// 1段目：出力順を先に確定させる
slices.sort_by(|a, b| b.value_jpy.cmp(&a.value_jpy).then_with(|| a.key.cmp(&b.key)));
// 2段目：端数の降順。sort_by は安定ソートなので、同値なら1段目の順序が残る
remainders.sort_by(|a, b| b.1.cmp(&a.1));
```

`Decimal::new(u, 2)` を使っているのはスケール2を型として保持するため。`Decimal::from(u) / Decimal::from(100)` だと 20% が `"20"` とシリアライズされ、`"33.34"` と桁が揃わない。

### 2.3 `group_by=none` は allocation では 422

時系列では「全体の推移」を見る正当な用途があるが、構成比で `none` を指定しても「1項目が100%」にしかならず情報量がゼロ。仕様の非対称（同じ enum が片方でだけ弾かれる）を許容し、ハンドラで明示的に検証した。

### 2.4 現金・預金残高を含まない旨を `scope` で明示

`transactions` は売買のみで、入出金テーブルは #8 の判断でスコープ外のまま。したがってこの構成比は「保有銘柄の評価額」であり、`account_type=bank` の口座があっても比率には現れない。

レスポンスに `"scope": "securities_only"` を持たせ、暗黙の前提にしないことにした。将来入出金を実装したときは `"scope": "total"` を返せるようになり、クライアント側で切り分けられる。

### 2.5 価格未登録の銘柄は分母から除外

#11 と同じ扱い。`unpriced_asset_count` として件数のみ返す。ここで方針を変えると2つのエンドポイントで合計額が食い違うため、揃えることが必須だった。

---

## 3. 詰まった点と教訓

今回は技術的な難所より、**作業プロセス上の事故**が支配的だった。実装そのものは3時間程度の内容だが、以下の4件で大幅に時間を溶かした。

### 3.1 差し替え指示を隣接する別の enum に適用した（最大の事故）

`analytics_service.rs` の `GroupBy` を差し替えるつもりが、直前にある `Granularity` の中身を上書きしてしまった。結果、`Granularity::Day` が消え、**原因から遠い箇所で21件のエラーが連鎖**した。

```
error: no variant named `Day` found for enum `Granularity`
error: no variant named `Account` found for enum `GroupBy`
...
```

エラーメッセージだけ見ると2つの enum が両方壊れているように読めるが、実体は「1回の貼り付けミス」。

**教訓：** 差し替え作業では、**置換前後の「最初の1行」が一致しているかを先に目視する**。同じファイル内に似た形の定義が並んでいる箇所では特に。

### 3.2 handler 用のコードを service ファイルに貼った

`handler/analytics.rs` に入れるべき `pub async fn allocation(State(st): State<AppState>, ...)` を `allocation_service.rs` に貼り付け、サービス側の関数シグネチャと本体の冒頭を上書きした。残された本体が `let mut slices = ...` から始まるため、`error: expected item, found keyword 'let'` という一見無関係なエラーになった。

**教訓：** `error: expected item, found <keyword>` は、ほぼ確実に**関数の外に関数本体が置かれている**サイン。構文ではなく貼り付け位置を疑う。

### 3.3 `pub mod` の書き忘れ（プロジェクト通算5回目）

新規ファイル `allocation_service.rs` を作ったが `src/service/mod.rs` への宣言を忘れ、`cannot find module allocation_service` が出た。

**教訓：** 新規ファイルを作った直後に、必ず以下を実行する。ファイル作成と `mod` 宣言を1つの操作として扱う。

```bash
grep -n '<新ファイル名>' src/<ディレクトリ>/mod.rs
```

### 3.4 `cargo fmt -- --check` が後続コマンドを止める

```bash
cargo fmt -- --check && cargo clippy ... && cargo test ...
```

整形差分があると `--check` が非ゼロで終了し、**`&&` で連結された test が一切走らない**。出力の見た目は「差分がずらっと出ただけ」なので、テストが通ったと誤認しかねなかった。

**教訓：** このワンライナーで差分が出たら、まず `cargo fmt`（`--check` なし）を実行してから再度回す。出力末尾に `test result:` が無ければテストは走っていない。

### 3.5 プロセス改善（今回から適用）

「このファイルの全文」として提示されたコードは、エディタでの全選択→貼り付けではなく**ヒアドキュメントで書き込む**運用に切り替えた。

```bash
cp src/service/target.rs /tmp/target.rs.bak
cat > src/service/target.rs << 'RSEOF'
（全文）
RSEOF
```

部分置換が「置換」ではなく「追加」として適用される事故が #6・#11・#12 で繰り返し起きているため。バックアップを取ってから上書きするので、`sed -i ''` の誤爆（#11）のような復旧不能な状態にもならない。

---

## 4. 検証結果

```
$ cargo fmt -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
```

| テスト対象 | 件数 | 結果 |
|---|---|---|
| `service::allocation_service`（ユニット） | 7 | ok |
| `domain::position`（ユニット、既存） | 10 | ok |
| `tests/analytics_test.rs` | 13（既存7＋新規6） | ok |
| その他統合テスト（accounts / assets / fx / holdings / transactions） | 40 | ok |

clippy は `-D warnings` で警告ゼロ。

### 4.1 ユニットテスト（`assign_ratios`）

| テスト | 検証内容 |
|---|---|
| `exact_division_keeps_ratios` | 割り切れる場合はそのままの比率 |
| `three_way_split_still_sums_to_100` | 3等分で `[33.34, 33.33, 33.33]`、合計 100.00 |
| `single_slice_is_100` | 1項目なら 100.00 |
| `scattered_remainders_sum_to_100` | 素数7件で端数が散っても合計 100.00 |
| `zero_value_slice_is_excluded` | 評価額ゼロは除外 |
| `empty_input_returns_empty` | 空入力・全ゼロで空を返す（0除算しない） |
| `slices_are_sorted_by_value_desc` | 評価額の降順 |

### 4.2 統合テスト

| テスト | 検証内容 |
|---|---|
| `allocation_ratios_sum_to_100` | **完了条件**。3銘柄同額で合計 100.00 |
| `allocation_by_account_returns_names` | 口座軸で `label` に口座名が入る |
| `allocation_matches_history_total` | asset-history の同日合計と `total_value_jpy` が一致 |
| `allocation_excludes_unpriced` | 価格未登録が分母から外れ、件数に計上 |
| `allocation_empty_portfolio` | 保有ゼロで 200・空配列 |
| `allocation_rejects_none_and_future` | `none`・未来日で 422、無認証で 401 |

---

## 5. 次タスクへの引き継ぎ

### 5.1 タスク#13 で使える資産

- `GroupBy` に `Account` / `Asset` が追加済み。asset-history 側でも銘柄別・口座別の時系列が引けるようになっている
- `HistorySeries.label` があるので、グラフの凡例を API 側から供給できる
- `assign_ratios` は純粋関数として独立しているので、他の比率表示（セクター別、通貨別など）にも再利用可能

### 5.2 未対応・将来の検討事項

| # | 項目 | 備考 |
|---|---|---|
| 1 | 現金・預金残高の反映 | 入出金テーブルの実装が前提。実装時は `scope` を `"total"` に切り替える |
| 2 | 上位N件＋「その他」への集約 | `group_by=asset` で銘柄数が多い場合。現状はフロント側の責務 |
| 3 | 目標配分との比較（リバランス支援） | 目標比率テーブルを持てば、`assign_ratios` の結果と差分を取るだけで実現できる |
| 4 | 外貨建て銘柄を含む allocation の統合テスト | 現状はJPY建てのみ。FX経路は #11 のテストがカバーしているが、allocation 経由の検証は無い |

4番は、`test_app_with_fx` を使えば追加できる。#11 の `foreign_asset_uses_trade_date_rate` が参考になる。

---

## 6. 再現コマンド

```bash
cd ~/workspace/shisan-api/asset-log

# ビルドと検証
cargo sqlx prepare -- --all-targets
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test --all-targets

# 手動確認（コンテナ起動後）
TOKEN=$(curl -s -X POST localhost:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"me@example.com","password":"***"}' | jq -r .access_token)

curl -s "localhost:8080/analytics/allocation?group_by=asset_class" \
  -H "authorization: Bearer $TOKEN" | jq

# 比率の合計が 100 になることの確認
curl -s "localhost:8080/analytics/allocation?group_by=asset" \
  -H "authorization: Bearer $TOKEN" \
  | jq '[.items[].ratio | tonumber] | add'
```

`.sqlx` は `fetch_trades_until` の SELECT 句変更に伴い更新されている。**コミットに含めること**（#11 で漏れた実績あり）。