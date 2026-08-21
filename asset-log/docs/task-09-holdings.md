# タスク#9 保有ポジション一覧 `GET /holdings`

## ゴールと完了条件

| 項目 | 内容 |
|---|---|
| ゴール | 保有ポジションを、現在価格を当てた評価損益つきで返す |
| 完了条件 | 評価損益が返ること（`tests/holdings_test.rs` green） |
| 結果 | 8ケース全green。全体でも44テスト（unit 10 / accounts 8 / assets 9 / holdings 8 / transactions 9）が通過 |

`domain::position`（#7）と取引CRUD（#8）の上に乗る読み取り専用エンドポイント。マイグレーションの追加は無し。

## 実装したもの

| ファイル | 役割 |
|---|---|
| `src/repository/holding_repo.rs` | 取引＋メタ情報を1本のSELECT、最新価格を `DISTINCT ON`、口座の存在確認 |
| `src/service/holdings_service.rs` | 畳み込み・評価・通貨ごと/口座ごとの集計。レスポンス型もここ |
| `src/handler/holdings.rs` | 入出力の変換のみ |
| `src/lib.rs` | `.route("/holdings", get(handler::holdings::list))` |
| `tests/holdings_test.rs` | 統合テスト8ケース（`.wip` から復帰） |

`.sqlx` は3件増えて23件。

## API仕様

```
GET /holdings?account_id=<uuid>&include_closed=<bool>
```

- `account_id` 省略時は全口座。他人の・存在しない口座は404
- `include_closed` 既定 `false`。`true` で数量0のポジションも含む

```json
{
  "holdings": [
    {
      "account_id": "...", "account_name": "特定口座", "account_type": "tokutei",
      "asset_id": "...", "symbol": "VOO", "name": "VOO テスト銘柄",
      "asset_class": "etf", "currency": "JPY", "price_unit": "1",
      "quantity": "10", "avg_cost": "500", "book_value": "5000", "realized_pnl": "0",
      "price": "550", "priced_on": "2026-08-20",
      "market_value": "5500", "unrealized_pnl": "500", "unrealized_pnl_rate": "0.1"
    }
  ],
  "summary": {
    "unpriced_count": 0,
    "totals":     [{ "currency": "JPY", "book_value": "5000", "market_value": "5500", "unrealized_pnl": "500", "unrealized_pnl_rate": "0.1", "realized_pnl": "0", "unpriced_count": 0 }],
    "by_account": [{ "account_id": "...", "account_name": "特定口座", "account_type": "tokutei", "totals": [ /* 同じ形 */ ] }]
  }
}
```

金額・数量は既存APIと同じく文字列。並び順は口座名 → シンボル。

## 設計判断の根拠

### 合計の騰落率は、分母を「評価できた銘柄の簿価」に分ける

価格が1件も登録されていない銘柄は、簿価の合計には含まれるが評価額には含まれない。合計の騰落率を `unrealized_pnl ÷ book_value` で出すと、未評価分だけ実際より低く出る。

`Accumulator` に `priced_book_value` を別に持ち、これを分母にした。テスト `unpriced_asset_returns_nulls` が直接ここを検証している（簿価6000・評価対象5000で、期待値は `500 ÷ 5000 = 0.1`。全簿価で割ると 0.0833 になって落ちる）。

### 合計は通貨ごとの配列にする

`/holdings` はJPY換算しない（#7で決めた「domainは資産通貨のまま」の延長）。JPYとUSDを足した単一の合計値は意味を持たないので、`Totals` に `currency` を持たせて配列で返す。単一フィールドにすると「数字が合わない」バグの温床になる。

### `?group_by=account` でトップレベルを分岐させない

口座ごとのネストを切り替え式にすると、レスポンススキーマが2種類に増える。#14のutoipa定義とフロントの型定義がどちらも二重になり、テストも両方必要になる。

フラット配列は常に不変にして、口座ごとの集計は `summary.by_account` に置いた。銘柄行に `account_id` / `account_name` があるので、ネスト表示が要るならフロント側で1行のgroupByで作れる。

### N+1を避け、畳み込みはRust側で行う

取引は1本のSELECTで全件取り、口座名・銘柄名・`price_unit`・通貨は同じクエリでJOINして持ってくる。`price_unit` は `build_holding` / `evaluate` の引数なので必須。

SQLの `ORDER BY` を `transactions_position_idx`（account_id, asset_id, traded_at, created_at, id）と同じ並びにしてあるので、同一ポジションの取引は必ず隣接する。`HashMap` ではなく `slice::chunk_by` の1回スキャンで分割でき、約定日昇順という `build_holding` の前提もそのまま満たされる。

最新価格は `DISTINCT ON (asset_id) ... ORDER BY asset_id, priced_on DESC`。`asset_prices` は `user_id` を持たない（#6の設計）ので `assets` 経由で絞る。

## つまずいた点と教訓

### `cargo check` の所要時間ではファイルの組み込み漏れを判別できない

**#8から3回繰り返した。** 新しいファイルを置いて `cargo check` が0.5秒で終わったとき、次の3つが区別できない。

1. 正しく組み込まれていて、前回のビルドが残っている
2. `mod` 宣言が無く、ファイルごと無視されている
3. そもそもファイルが0バイト（#1の雛形作成スクリプトが作った空ファイルが残っていた）

今回は3つとも実際に起きた。判別に使えるのは所要時間ではなく、以下の直接確認だけ。

```bash
newfile() {
  wc -c "src/$1" && grep -rn "$(basename "$1" .rs)" src/*/mod.rs src/lib.rs
}
```

`~/.zshrc` に入れて `newfile handler/holdings.rs` で、ファイルの中身・`mod` 宣言・ルータ登録の3点が一度に見える。強制的に再コンパイルさせたいときは `touch src/lib.rs && cargo check --all-targets`。

ルータ登録の漏れは**コンパイルが通ってしまい、実行時に404になるだけ**なので、テストを書き始めてから気づくと原因の切り分けに時間を取られる。

### `AppError::NotFound` はリソース名だけを渡す

定義が `#[error("{0}が見つかりません")] NotFound(&'static str)` なので、`NotFound("口座が見つかりません")` と書くと「口座が見つかりませんが見つかりません」になる。正しくは `NotFound("口座")`。

### `AppError::Internal` は `anyhow::Error` を包む

`Internal(#[from] anyhow::Error)` なので `AppError::Internal(anyhow::anyhow!("..."))`。5xxは `trace_id` しか返さないため、詳細は `tracing::error!` でログに出す。

`build_holding` の失敗はここに落としている。売り数量超過は#8のハンドラで弾いているので、ここに来たらDBデータの不整合。

## テスト8ケース

| テスト | 検証内容 |
|---|---|
| `holdings_returns_valuation` | 完了条件。価格を2日分入れ、`DISTINCT ON` が最新日を拾うことも確認 |
| `mutual_fund_valuation_uses_price_unit` | 投資信託の10,000口あたり基準価額。素通ししていたら1万倍ずれる |
| `unpriced_asset_returns_nulls` | 価格未登録は行が残り評価系がnull。騰落率の分母 |
| `closed_position_is_hidden_by_default` | 既定で非表示、`include_closed=true` で実現損益つきで出る |
| `totals_are_split_by_currency` | JPYとUSDが別の合計になる |
| `positions_are_grouped_per_account` | 同一銘柄でも口座が違えば別行。`by_account` の合計とフラット配列の合計が一致 |
| `account_filter_and_unknown_account_is_404` | 絞り込み、他人の口座も存在しない口座も404 |
| `requires_authentication` | トークン無しで401 |

## 次タスクへの引き継ぎ

タスク#10は `FxRateProvider`（Frankfurter API連携）。

- `/holdings` の `summary.totals` は既に通貨ごとの配列になっている。JPY建ての単一合計は `analytics_service` 側で、この配列にレートを掛けて作る形になる
- 外貨建て資産をdomainでは資産通貨のまま計算する方針は#7から一貫している。換算をどこか一箇所に閉じ込めておくこと
- `Query` のパース失敗（`account_id=abc` など）はaxumの既定リジェクションが返り、`AppError` を通らないためRFC 9457形式にならない。既存ハンドラも同様。#14で気になったらカスタム抽出子で `AppError::BadRequest` に寄せる

## 再現コマンド

```bash
cd ~/workspace/shisan-api && docker compose up -d db && cd asset-log
cargo test --test holdings_test
cargo clippy --all-targets -- -D warnings
cargo test
```