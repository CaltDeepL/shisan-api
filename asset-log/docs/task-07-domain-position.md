# タスク#7: domain::position（総平均法・評価損益）

## ゴールと完了条件

| 項目 | 内容 |
|---|---|
| ゴール | 取引履歴から保有ポジション（数量・平均取得単価・簿価・実現損益）を求め、現在価格を当てて評価損益を出す純粋関数レイヤーを作る |
| 完了条件 | ユニットテスト8ケースgreen・DB不要 |
| 結果 | **達成**。10ケース全green（完了条件の8件 + 防御的テスト2件）、コンパイル警告なし |
| 成果物 | `src/domain/position.rs`（実装 + `#[cfg(test)]` のユニットテスト） |

## 公開API

```rust
pub fn build_holding(trades: &[Trade], price_unit: Decimal) -> Result<Holding, PositionError>;
pub fn evaluate(holding: &Holding, price: Decimal, price_unit: Decimal) -> Valuation;
```

| 型 | 役割 |
|---|---|
| `TradeKind { Buy, Sell }` | 取引種別 |
| `Trade { kind, quantity, price, fee }` | 入力。約定日時の昇順で渡す前提 |
| `Holding { quantity, avg_cost, book_value, realized_pnl }` | 取引列を畳み込んだ結果 |
| `Valuation { market_value, book_value, unrealized_pnl, unrealized_pnl_rate, realized_pnl }` | 現在価格を当てた評価結果 |
| `PositionError` | Oversell / NonPositiveQuantity / NegativePrice / NegativeFee / NonPositivePriceUnit |

## 計算ルール

- **買い**: 簿価に `数量 × 単価 ÷ price_unit + 手数料` を加算
- **売り**: 平均単価は変えず、数量と簿価のみ減らす。`実現損益 += (売却代金 − 手数料) − 按分した取得原価`
- **全売却**: 数量・簿価・平均単価を0にリセット。再購入時は新規分だけで平均を作り直す
- **評価額**: `数量 × 現在価格 ÷ price_unit`
- **騰落率**: 簿価0のときは `None`（ゼロ除算回避）

`price_unit` は `assets.price_unit`。株式・ETFは `1`、投資信託は `10000`（10,000口あたりの基準価額）。
`avg_cost` は画面上で現在価格と直接比較できるよう、**price_unit あたりの単価**（呼値と同じ土俵）で保持する。

## 設計判断の根拠

| 論点 | 採用 | 理由 |
|---|---|---|
| 丸め | 内部は `Decimal` のまま、丸めなし | 税務用の切上げは表示/レポート層の責務。ここで丸めると買い増しのたびに誤差が累積する |
| 平均単価の保持 | 毎回 `簿価 ÷ 数量` から引き直し | 逐次更新だと丸め誤差が蓄積する。簿価を唯一の真実として扱う |
| 売却時の取得原価 | `簿価 × 売却割合` で按分 | `avg_cost × 数量` だと割り切れない場合に簿価へ端数が残る |
| 全売却時 | 簿価・平均単価を明示的に0にリセット | 按分の端数が残るのを防ぐ |
| 手数料 | スコープに含める | 後付けだと平均単価の計算式ごと書き換えになるため |
| 外貨建て | domain は資産通貨のまま計算 | JPY換算は `analytics_service` の責務。domain を為替に依存させない |
| `price_unit` の引き回し | `build_holding` / `evaluate` の両方に渡す | 簿価と評価額が同じ単位系でないと含み損益が壊れる。片方だけでは不可 |

## つまずいた点と教訓

- 当初の設計では `Trade` に `traded_at` を持たせる想定だったが、計算に一切使わない（並び順は repository の `ORDER BY` が保証する）ため**外した**。domain に持たせるとテストのたびに意味のない日時を作る必要が出るだけだった。
  - ただし将来 FIFO 法・個別法を追加する場合は戻すことになる。
- 売却時の取得原価を `avg_cost × 売却数量` で計算すると、平均単価が割り切れないケース（例: 3口を1000円で買って1口売却）で簿価に端数が残り、全売却しても簿価が0にならない。**簿価から直接按分**する方式に変更した。

## 次タスクへの引き継ぎ

- `PositionError` → `AppError` の変換はタスク#9（`/holdings`）で `error.rs` に追加する:
  ```rust
  impl From<PositionError> for AppError {
      fn from(e: PositionError) -> Self { AppError::unprocessable(e.to_string()) }
  }
  ```
- `Oversell` は本来**タスク#8（取引CRUD）の登録時バリデーションで弾くべき**もの。`/holdings` の読み取り時に発生するなら「DBに矛盾したデータが入っている」というシグナルになる。
  - タスク#8で「保有数量を超える売却取引の登録を422で拒否する」統合テストを入れること。
- タスク#9では repository から `ORDER BY traded_at, id` で取引を取得し、`assets.price_unit` と併せて `build_holding` に渡す。

## 再現コマンド

```bash
cd ~/workspace/shisan-api/asset-log
cargo test --lib domain::position
```

依存関係（追加分）:

```toml
[dependencies]
thiserror = "2"

[dev-dependencies]
rust_decimal_macros = "1"  # dec! マクロ、テスト専用
```