# タスク#11 資産推移 API（GET /analytics/asset-history）

完了条件: **欠損日が補完される**

## 追加・変更したファイル

| 区分 | ファイル | 内容 |
|---|---|---|
| マイグレーション | なし | 既存の transactions / asset_prices / fx_rates で組める |
| repository | `src/repository/analytics_repo.rs`（新規） | 取引取得 + 日付 × 銘柄の価格グリッド |
| repository | `src/repository/fx_repo.rs` | `coverage` / `find_in_range` / `upsert_many` を追加 |
| provider | `src/provider/fx.rs` | `FxRatePoint` と `rates_in_range` を追加 |
| provider | `src/provider/cached_fx.rs` | `rates_in_range` を素通し実装 |
| domain | `src/domain/position.rs` | `apply_trade` を抽出（`build_holding` はその畳み込みに再定義） |
| service | `src/service/fx_history.rs`（新規） | 為替キャッシュの被覆判定と補充 |
| service | `src/service/analytics_service.rs`（新規） | 畳み込み・評価・グルーピング |
| handler | `src/handler/analytics.rs`（新規） | クエリ受け取りとバリデーション |
| tests | `tests/analytics_test.rs`（新規） | 統合テスト7ケース |

## エンドポイント

```http
GET /analytics/asset-history?from=2026-01-01&to=2026-08-16&granularity=day&group_by=account_type
```

| パラメータ | 既定値 | 値 |
|---|---|---|
| `from` | `to` の365日前 | 日付 |
| `to` | JSTの今日 | 日付。未来日は422 |
| `granularity` | `day` | `day` / `month` |
| `group_by` | `none` | `none` / `account_type` / `asset_class` |

```json
{
  "granularity": "day",
  "base_currency": "JPY",
  "fx_stale": false,
  "series": [
    {
      "key": "total",
      "points": [
        { "date": "2026-08-10", "market_value_jpy": "5500", "cost_jpy": "5000", "unpriced_asset_count": 0 }
      ]
    }
  ]
}
```

期間上限は2000点。`from > to`・未来日・期間超過はいずれも422で、`errors[0].field` に `from` / `to` が入る。

## 設計判断

### 1. 履歴はスナップショットではなく都度再計算

タスク#14（日次スナップショットバッチ）より前にこのタスクが来ているが、順番の都合ではなく設計として都度計算が正しい。

- **過去日入力に耐える** — タスク#8で backdated な取引登録を許容した。スナップショット由来だと、後から1月の取引を入れても1月の系列は古いまま
- **稼働前の期間を返せる** — バッチ稼働以前の資産推移はスナップショットに存在しない

タスク#14は「このAPIの結果をキャッシュする層」という位置づけになる。

### 2. 欠損日の補完は2種類ある

| 穴 | 原因 | 埋め方 |
|---|---|---|
| その日に取引が無い | 大半の日 | 数量・簿価を前日から引き継ぐ（Rust側の畳み込み） |
| その日に価格が無い | 土日祝、投信の基準価額未反映 | 直近営業日の価格を引き継ぐ（SQL側の LATERAL） |

### 3. 総平均法の再実装をしない

`domain::position` の `build_holding` は「取引の列 → 最終状態」なので、日ごとに呼ぶと O(D × T) になる。ループ本体を `apply_trade` として抽出し、1パスで進めながら取引日ごとに `Holding` を複製する形にした（O(T + D·P·log T)）。

`build_holding` のシグネチャは変えていないので、タスク#7のユニットテスト10ケースとタスク#8・#9の呼び出し側は無改修。SQLでウィンドウ関数を書く案は、テスト済みロジックの二重管理になるため却下。

### 4. 外貨は「取引を組む時点でJPYに寄せる」

簿価は**約定日のレート**、時価は**その日のレート**で換算する（会計上、取得原価は取得時点で確定するため）。

実装は `Trade` を組み立てる時点で `price` と `fee` をJPY換算するだけで済む。`gross = quantity × price_jpy ÷ price_unit` となり `book_value` が最初からJPY建てになるので、`domain::position` に為替を持ち込まずに済んだ。副作用として `avg_cost` もJPY建てになるが、`/holdings` は既存の呼び出しのままなので影響しない。

結果として評価損益に為替差損益が含まれる。テストでは 110USD × 160円 = 17,600円（時価）に対し簿価15,000円で、含み益2,600円のうち1,600円が為替差益。

### 5. 価格・レートが引けない銘柄

評価額の合計から除外し、各点に `unpriced_asset_count` を添える。`HashSet<Uuid>` で数えているので、同じ銘柄を複数口座で持っていても1と数える。

- その日の価格が無い（`AssetClass::is_priceable` が false の現金は対象外。額面評価する）
- 約定日のレートが引けない → そのポジション全体が計算不能なので、数量が正である全ての日で計上

簿価も一緒に除外する。片方だけ残すと損益が実態とかけ離れる。

## ハマった点

### `LEFT JOIN LATERAL` の nullable は sqlx が推論できない ★

**今回いちばん重要な発見。**

```sql
LEFT JOIN LATERAL (
    SELECT price, priced_on FROM asset_prices
    WHERE asset_id = h.asset_id AND priced_on <= sp.on_date
    ORDER BY priced_on DESC LIMIT 1
) p ON true
```

`.sqlx` の JSON を見ると `price` の `nullable` が **false** で返る。sqlx は外部結合の性質を解析できず、`asset_prices.price` の NOT NULL 制約をそのまま持ち上げている。

`p.price` と素直に書くと型が `Decimal` になり、**価格未登録の銘柄に当たった瞬間に実行時パニック**（`UnexpectedNullError`）。コンパイルは通り、テストデータに価格が揃っていれば統合テストも通り、本番で落ちる。

→ `AS "price?"` / `AS "priced_on?"` の明示が**必須**。

確認方法:

```bash
f=$(grep -l "generate_series" .sqlx/*.json | head -1)
python3 -c "
import json
d = json.load(open('$f'))
for c, n in zip(d['describe']['columns'], d['describe']['nullable']):
    print(f\"{c['name']:12} {c['type_info']:12} nullable={n}\")
"
```

### 月末の生成で `generate_series` を月刻みにしない

```
generate_series('2026-01-31', '2026-08-31', '1 month')
→ 1/31, 2/28, 3/28, 4/28, ...   ← 3月以降が月末に戻らない
```

日次で回して月末日を絞り込む形にすれば、うるう年も含めて計算不要。

```sql
SELECT d::date AS on_date
FROM generate_series($2::date, $3::date, interval '1 day') AS d
WHERE $4::text = 'day'
   OR d::date = $2::date                                        -- 期間の始点
   OR d::date = $3::date                                        -- 期間の終点
   OR d::date = (date_trunc('month', d) + interval '1 month - 1 day')::date
```

`from` / `to` を条件に含めているのは、月末以外から始まる期間でもグラフの両端が立つようにするため。フィルタは CROSS JOIN より先に効くので、月次でも LATERAL は残った日付ぶんしか実行されない。

### 為替キャッシュの中抜けを `StalePolicy` では検出できない

`CachedFxProvider` の `rate()` は単日の read-through として完結しているが、`StalePolicy::accepts` は単日の鮮度しか見ない。1月と8月のレートだけ持っている状態で通年を要求すると「シードもあるし末尾も新しい」と判定され、**1月末のレートが8月まで水平に伸びたグラフ**が返る。値が返るのでテストでも気づきにくい。

そこで `fx_history::load` に3条件の判定を置き、デコレータ側の `rates_in_range` は素通しにした。

| 条件 | 意図 |
|---|---|
| `seed_on` が NULL | `from` 以前に1件も無い＝起点が引けない |
| `max_gap > 7` | 期間の中抜け |
| `newest_on < to - 4日` | 末尾が古い |

閾値7日の根拠は、ECBの最長連休がイースター（金〜月）で連続レート間が最大5日空くため。

この判定は統合テストで実際に発動した。`foreign_asset_uses_trade_date_rate` の初版で 8/1 と 8/10 の2件だけシードしたところ、`max_gap` が9日で補充が走り `fx_stale = true` になった。**検出ロジックとしては正しい動作**なので、テスト側を日次で埋める形に直した。

### `pub mod` 宣言漏れ（タスク#8から通算3回目）

`cargo check` が「Checking」を出さずに1秒未満で終わったら、新規ファイルが `mod.rs` に登録されていない。今回は `analytics_repo` / `fx_history` / `analytics_service` の3つで発生。

### `sed -i ''` の実行先ミス

`src/handler/mod.rs` のつもりで `sed -i '' '$d'` を走らせ、`pub mod user_repository;` を `handler/mod.rs` 側に追記してしまい、`handler/auth.rs` の `use` 行まで巻き添えで消えた。破壊的な編集の後は `git diff` で意図しない差分が無いか確認する。

## テスト（tests/analytics_test.rs 7ケース）

| ケース | 検証内容 |
|---|---|
| `fills_missing_dates` | **完了条件そのもの**。価格1日分から7日分の点が立ち、直近価格が横に伸びる |
| `zero_before_first_trade_and_after_full_sell` | 取引前・全売却後も点は消えず0で出る |
| `monthly_returns_month_ends` | 月末＋期間両端。`1/15, 1/31, 2/28, 3/31, 4/10` |
| `unpriced_asset_is_counted_not_valued` | 価格未登録は評価額・簿価とも除外し、`unpriced_asset_count` に計上 |
| `group_by_account_type_splits_series` | 系列が口座区分で分かれる |
| `foreign_asset_uses_trade_date_rate` | 簿価は約定日レート、時価は当日レート |
| `invalid_range_and_auth` | 未来日・期間逆転が422、未認証が401 |

`test_app` の FX ベースURLは到達不能なので、JPY建てのケースでは外部呼び出しが一切走らない。走ってしまったらそれ自体がバグの兆候。

## 再現手順

```bash
cd ~/workspace/shisan-api && docker compose up -d db && cd asset-log
touch src/lib.rs && cargo check --all-targets
cargo sqlx prepare -- --all-targets
git status --short .sqlx        # 増分の確認
cargo test --test analytics_test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## 次のタスクへの申し送り

**タスク#12（GET /analytics/allocation、完了条件は比率合計が100%）**

- `fx_history::load` と `apply_trade` の畳み込みがそのまま使える。必要なのは特定の1日のポジションだけなので `from = to = as_of` に近い形になる
- 丸めが山場。今回は `round_dp(0)` を最後に一度だけかけたが、比率は各項目を丸めると合計が99.99%になる。最大剰余法などで端数を1項目に寄せる処理が要る

**タスク#14（日次スナップショットバッチ）**

- このAPIのキャッシュ層として設計する。過去日入力があった日は再計算が要る点に注意
- `apply_trade` はここでも使える

**設定値の重複**

`fx_history::MAX_TAIL_DAYS = 4` は `StalePolicy::default()` の `max_calendar_days` と同じ値を別々に持っている。`AppState` が `config` を保持していないため。タスク#15で設定を整理する際に一本化する。