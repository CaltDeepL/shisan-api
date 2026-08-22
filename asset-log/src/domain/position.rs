//! 保有ポジションの計算（総平均法・評価損益）。
//!
//! このモジュールは **純粋関数のみ** で構成する。DB・時刻・設定に依存しない。
//! 取引の並び順（約定日時の昇順）は呼び出し側（repository の ORDER BY）が保証する。
//!
//! ## 金額の単位について
//! `price_unit` は「呼値が何口あたりか」を表す（`assets.price_unit`）。
//! - 株式・ETF: `1`（1株あたりの株価）
//! - 投資信託 : `10000`（10,000口あたりの基準価額）
//!
//! 実際の金額は常に `数量 × 価格 ÷ price_unit` で求める。
//! 一方 `avg_cost` は画面上で現在価格と直接比較できるよう、
//! **price_unit あたりの単価**（＝呼値と同じ土俵）で保持する。

use rust_decimal::Decimal;

/// 取引種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "trade_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    Buy,
    Sell,
}

/// 1件の取引。約定日時は計算に不要なため domain では持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    pub kind: TradeKind,
    /// 数量（口数・株数）。正の数のみ。
    pub quantity: Decimal,
    /// 約定単価（price_unit あたり）。負は不可。
    pub price: Decimal,
    /// 手数料。買いは取得費に加算、売りは売却代金から控除。負は不可。
    pub fee: Decimal,
}

impl Trade {
    pub fn buy(quantity: Decimal, price: Decimal, fee: Decimal) -> Self {
        Self {
            kind: TradeKind::Buy,
            quantity,
            price,
            fee,
        }
    }

    pub fn sell(quantity: Decimal, price: Decimal, fee: Decimal) -> Self {
        Self {
            kind: TradeKind::Sell,
            quantity,
            price,
            fee,
        }
    }
}

/// 取引列を畳み込んだ結果の保有状態。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Holding {
    /// 現在の保有数量。
    pub quantity: Decimal,
    /// 平均取得単価（price_unit あたり）。保有数量0のときは0。
    pub avg_cost: Decimal,
    /// 簿価（取得原価の残高）。保有数量0のときは0。
    pub book_value: Decimal,
    /// 実現損益の累計（売却時に確定した損益の合計）。
    pub realized_pnl: Decimal,
}

impl Holding {
    /// 保有なし（全売却済み or 取引ゼロ件）かどうか。
    pub fn is_closed(&self) -> bool {
        self.quantity.is_zero()
    }
}

/// 現在価格を当てた評価結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valuation {
    /// 評価額（数量 × 現在価格 ÷ price_unit）。
    pub market_value: Decimal,
    /// 簿価。
    pub book_value: Decimal,
    /// 評価損益（含み損益）。
    pub unrealized_pnl: Decimal,
    /// 騰落率。簿価0のときは None（ゼロ除算回避）。
    pub unrealized_pnl_rate: Option<Decimal>,
    /// 実現損益の累計。
    pub realized_pnl: Decimal,
}

/// ポジション計算のエラー。handler 層で 422 にマップする想定。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PositionError {
    #[error("売却数量 {requested} が保有数量 {held} を超えています")]
    Oversell { requested: Decimal, held: Decimal },

    #[error("数量は正の数である必要があります (指定値: {0})")]
    NonPositiveQuantity(Decimal),

    #[error("価格は0以上である必要があります (指定値: {0})")]
    NegativePrice(Decimal),

    #[error("手数料は0以上である必要があります (指定値: {0})")]
    NegativeFee(Decimal),

    #[error("price_unit は正の数である必要があります (指定値: {0})")]
    NonPositivePriceUnit(Decimal),
}

/// 取引列を総平均法で畳み込み、保有状態を求める。
///
/// - 買い: 簿価に `数量 × 単価 ÷ price_unit + 手数料` を加算し、平均単価を引き直す
/// - 売り: 平均単価は変えず、数量と簿価のみ減らす。差額を実現損益に積む
/// - 全売却: 数量・簿価・平均単価を0にリセットする（再購入時は新規分だけで平均を作り直す）
pub fn build_holding(trades: &[Trade], price_unit: Decimal) -> Result<Holding, PositionError> {
    if price_unit <= Decimal::ZERO {
        return Err(PositionError::NonPositivePriceUnit(price_unit));
    }

    let mut holding = Holding::default();
    for trade in trades {
        apply_trade(&mut holding, trade, price_unit)?;
    }
    Ok(holding)
}

/// 保有状態を1取引ぶん進める。`build_holding` はこれの畳み込みとして定義される。
///
/// 日付ごとのスナップショットが欲しい呼び出し側（資産推移・日次バッチ）は、
/// 取引列を1パスで舐めながら日付境界で `Holding` を複製する。
pub fn apply_trade(
    holding: &mut Holding,
    trade: &Trade,
    price_unit: Decimal,
) -> Result<(), PositionError> {
    if price_unit <= Decimal::ZERO {
        return Err(PositionError::NonPositivePriceUnit(price_unit));
    }
    validate(trade)?;

    // 呼値 → 実際の金額へ換算
    let gross = trade.quantity * trade.price / price_unit;

    match trade.kind {
        TradeKind::Buy => {
            holding.quantity += trade.quantity;
            holding.book_value += gross + trade.fee;
        }
        TradeKind::Sell => {
            if trade.quantity > holding.quantity {
                return Err(PositionError::Oversell {
                    requested: trade.quantity,
                    held: holding.quantity,
                });
            }

            // 売却分の取得原価は「簿価 × 売却割合」で按分する。
            // avg_cost 経由で計算すると割り切れない場合に誤差が残るため、
            // 簿価から直接按分して簿価の整合性を保つ。
            let cost_of_sold = holding.book_value * trade.quantity / holding.quantity;
            let proceeds = gross - trade.fee;

            holding.realized_pnl += proceeds - cost_of_sold;
            holding.quantity -= trade.quantity;
            holding.book_value -= cost_of_sold;
        }
    }

    // 平均単価は毎回 簿価 ÷ 数量 から引き直す（逐次更新による誤差の蓄積を避ける）
    holding.avg_cost = if holding.quantity.is_zero() {
        holding.book_value = Decimal::ZERO; // 全売却時は端数を残さずリセット
        Decimal::ZERO
    } else {
        holding.book_value / holding.quantity * price_unit
    };

    Ok(())
}

/// 保有状態に現在価格を当てて評価損益を求める。
pub fn evaluate(holding: &Holding, price: Decimal, price_unit: Decimal) -> Valuation {
    let market_value = if price_unit.is_zero() {
        Decimal::ZERO
    } else {
        holding.quantity * price / price_unit
    };

    let unrealized_pnl = market_value - holding.book_value;

    let unrealized_pnl_rate = if holding.book_value.is_zero() {
        None
    } else {
        Some(unrealized_pnl / holding.book_value)
    };

    Valuation {
        market_value,
        book_value: holding.book_value,
        unrealized_pnl,
        unrealized_pnl_rate,
        realized_pnl: holding.realized_pnl,
    }
}

fn validate(trade: &Trade) -> Result<(), PositionError> {
    if trade.quantity <= Decimal::ZERO {
        return Err(PositionError::NonPositiveQuantity(trade.quantity));
    }
    if trade.price < Decimal::ZERO {
        return Err(PositionError::NegativePrice(trade.price));
    }
    if trade.fee < Decimal::ZERO {
        return Err(PositionError::NegativeFee(trade.fee));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// 株式・ETF: 1株あたりの呼値
    const UNIT_SHARE: Decimal = Decimal::ONE;

    /// 投資信託: 10,000口あたりの基準価額
    fn unit_fund() -> Decimal {
        dec!(10000)
    }

    // ケース1: 単一の買い
    #[test]
    fn single_buy_sets_quantity_and_avg_cost() {
        let trades = vec![Trade::buy(dec!(100), dec!(1000), Decimal::ZERO)];

        let holding = build_holding(&trades, UNIT_SHARE).unwrap();

        assert_eq!(holding.quantity, dec!(100));
        assert_eq!(holding.avg_cost, dec!(1000));
        assert_eq!(holding.book_value, dec!(100000));
        assert_eq!(holding.realized_pnl, Decimal::ZERO);
    }

    // ケース2: 単価違いの買い増しで加重平均になる
    #[test]
    fn additional_buy_produces_weighted_average() {
        let trades = vec![
            Trade::buy(dec!(100), dec!(1000), Decimal::ZERO),
            Trade::buy(dec!(100), dec!(1400), Decimal::ZERO),
        ];

        let holding = build_holding(&trades, UNIT_SHARE).unwrap();

        assert_eq!(holding.quantity, dec!(200));
        assert_eq!(holding.avg_cost, dec!(1200)); // (100000 + 140000) / 200
        assert_eq!(holding.book_value, dec!(240000));
    }

    // ケース3: 一部売却では平均取得単価が変わらない
    #[test]
    fn partial_sell_keeps_avg_cost_and_realizes_pnl() {
        let trades = vec![
            Trade::buy(dec!(100), dec!(1000), Decimal::ZERO),
            Trade::buy(dec!(100), dec!(1400), Decimal::ZERO),
            Trade::sell(dec!(50), dec!(1500), Decimal::ZERO),
        ];

        let holding = build_holding(&trades, UNIT_SHARE).unwrap();

        assert_eq!(holding.quantity, dec!(150));
        assert_eq!(
            holding.avg_cost,
            dec!(1200),
            "売却では平均取得単価は変わらない"
        );
        assert_eq!(holding.book_value, dec!(180000)); // 240000 - 60000
        assert_eq!(holding.realized_pnl, dec!(15000)); // 75000 - 60000
    }

    // ケース4: 全部売却で数量・簿価が0になり、実現損益が累計される
    #[test]
    fn full_sell_closes_position() {
        let trades = vec![
            Trade::buy(dec!(100), dec!(1000), Decimal::ZERO),
            Trade::sell(dec!(40), dec!(1200), Decimal::ZERO),
            Trade::sell(dec!(60), dec!(900), Decimal::ZERO),
        ];

        let holding = build_holding(&trades, UNIT_SHARE).unwrap();

        assert!(holding.is_closed());
        assert_eq!(holding.quantity, Decimal::ZERO);
        assert_eq!(holding.book_value, Decimal::ZERO);
        assert_eq!(holding.avg_cost, Decimal::ZERO);
        // (48000 - 40000) + (54000 - 60000) = 8000 - 6000
        assert_eq!(holding.realized_pnl, dec!(2000));
    }

    // ケース5: 全売却後の再購入では平均取得単価がリセットされる
    #[test]
    fn rebuy_after_full_sell_resets_avg_cost() {
        let trades = vec![
            Trade::buy(dec!(100), dec!(1000), Decimal::ZERO),
            Trade::sell(dec!(100), dec!(1200), Decimal::ZERO),
            Trade::buy(dec!(10), dec!(3000), Decimal::ZERO),
        ];

        let holding = build_holding(&trades, UNIT_SHARE).unwrap();

        assert_eq!(holding.quantity, dec!(10));
        assert_eq!(holding.avg_cost, dec!(3000), "過去の平均単価を引きずらない");
        assert_eq!(holding.book_value, dec!(30000));
        assert_eq!(holding.realized_pnl, dec!(20000));
    }

    // ケース6: 手数料は買いで取得費に加算、売りで実現損益から控除される
    #[test]
    fn fee_is_added_on_buy_and_deducted_on_sell() {
        let trades = vec![
            Trade::buy(dec!(100), dec!(1000), dec!(500)),
            Trade::sell(dec!(50), dec!(1200), dec!(300)),
        ];

        let holding = build_holding(&trades, UNIT_SHARE).unwrap();

        // 買い: 簿価 100000 + 500 = 100500 → 平均単価 1005
        assert_eq!(holding.avg_cost, dec!(1005));
        assert_eq!(holding.quantity, dec!(50));
        assert_eq!(holding.book_value, dec!(50250));
        // 売り: (60000 - 300) - 50250
        assert_eq!(holding.realized_pnl, dec!(9450));
    }

    // ケース7: price_unit の違い（株式 / 投資信託）で評価額が正しく出る
    #[test]
    fn evaluate_handles_price_unit_for_shares_and_funds() {
        // 株式: 1株1000円で100株 → 現在1250円
        let shares = build_holding(
            &[Trade::buy(dec!(100), dec!(1000), Decimal::ZERO)],
            UNIT_SHARE,
        )
        .unwrap();
        let v = evaluate(&shares, dec!(1250), UNIT_SHARE);

        assert_eq!(v.market_value, dec!(125000));
        assert_eq!(v.unrealized_pnl, dec!(25000));
        assert_eq!(v.unrealized_pnl_rate, Some(dec!(0.25)));

        // 投信: 基準価額12000円(10,000口あたり)で50,000口 → 現在15000円
        let fund = build_holding(
            &[Trade::buy(dec!(50000), dec!(12000), Decimal::ZERO)],
            unit_fund(),
        )
        .unwrap();
        let v = evaluate(&fund, dec!(15000), unit_fund());

        assert_eq!(fund.book_value, dec!(60000));
        assert_eq!(v.market_value, dec!(75000));
        assert_eq!(v.unrealized_pnl, dec!(15000));
        assert_eq!(v.unrealized_pnl_rate, Some(dec!(0.25)));
    }

    // ケース8: 保有数量を超える売却はエラー
    #[test]
    fn oversell_is_rejected() {
        let trades = vec![
            Trade::buy(dec!(100), dec!(1000), Decimal::ZERO),
            Trade::sell(dec!(101), dec!(1000), Decimal::ZERO),
        ];

        let err = build_holding(&trades, UNIT_SHARE).unwrap_err();

        assert_eq!(
            err,
            PositionError::Oversell {
                requested: dec!(101),
                held: dec!(100)
            }
        );
    }

    // --- 以下は完了条件の8ケースに対する追加の防御的テスト ---

    #[test]
    fn empty_trades_produce_zero_holding() {
        let holding = build_holding(&[], UNIT_SHARE).unwrap();

        assert_eq!(holding, Holding::default());
        assert_eq!(
            evaluate(&holding, dec!(1000), UNIT_SHARE).unrealized_pnl_rate,
            None
        );
    }

    #[test]
    fn non_positive_quantity_is_rejected() {
        let trades = vec![Trade::buy(Decimal::ZERO, dec!(1000), Decimal::ZERO)];

        assert_eq!(
            build_holding(&trades, UNIT_SHARE).unwrap_err(),
            PositionError::NonPositiveQuantity(Decimal::ZERO)
        );
    }
}
