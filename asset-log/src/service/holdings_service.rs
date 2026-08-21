//! 保有ポジション一覧の組み立て。
//!
//! repository が返す取引の平坦なリストを (account_id, asset_id) ごとに畳み込み、
//! 最新価格を当てて評価損益を出し、通貨ごと・口座ごとの合計を集計する。

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::AccountType;
use crate::domain::asset::AssetClass;
use crate::domain::position::{Trade, build_holding, evaluate};
use crate::error::AppError;
use crate::repository::holding_repo::{self, LatestPriceRow, TradeRow};

/// `GET /holdings` のクエリパラメータ。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HoldingsQuery {
    /// 指定した口座のみに絞る。他人の・存在しない口座は 404。
    pub account_id: Option<Uuid>,
    /// `true` で全売却済み（数量0）のポジションも返す。実現損益の集計用。
    #[serde(default)]
    pub include_closed: bool,
}

/// 保有1行。銘柄 × 口座の単位。
#[derive(Debug, Clone, Serialize)]
pub struct HoldingItem {
    pub account_id: Uuid,
    pub account_name: String,
    pub account_type: AccountType,
    pub asset_id: Uuid,
    pub symbol: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub currency: String,
    pub price_unit: Decimal,

    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub book_value: Decimal,
    pub realized_pnl: Decimal,

    /// 価格が1件も登録されていない銘柄では、以下4つはすべて `null`。
    pub price: Option<Decimal>,
    pub priced_on: Option<NaiveDate>,
    pub market_value: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
    pub unrealized_pnl_rate: Option<Decimal>,
}

/// 通貨ごとの合計。
///
/// `/holdings` は JPY 換算しないため、合計は必ず通貨単位で分ける
/// （JPY と USD を足した単一の数値は意味を持たない）。
#[derive(Debug, Clone, Serialize)]
pub struct Totals {
    pub currency: String,
    /// 全保有の簿価（価格未登録の銘柄も含む）。
    pub book_value: Decimal,
    /// 価格のある銘柄のみの評価額。
    pub market_value: Decimal,
    pub unrealized_pnl: Decimal,
    /// `unrealized_pnl ÷ 評価できた分の簿価`。分母が0なら `null`。
    pub unrealized_pnl_rate: Option<Decimal>,
    pub realized_pnl: Decimal,
    /// この通貨のうち、価格が無く評価対象外になった件数。
    pub unpriced_count: usize,
}

/// 口座ごとの内訳。
#[derive(Debug, Clone, Serialize)]
pub struct AccountSummary {
    pub account_id: Uuid,
    pub account_name: String,
    pub account_type: AccountType,
    pub totals: Vec<Totals>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HoldingsSummary {
    /// 全体で価格が無く評価対象外になった件数。
    pub unpriced_count: usize,
    pub totals: Vec<Totals>,
    pub by_account: Vec<AccountSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HoldingsResponse {
    pub holdings: Vec<HoldingItem>,
    pub summary: HoldingsSummary,
}

pub async fn list_holdings(
    db: &PgPool,
    user_id: Uuid,
    query: HoldingsQuery,
) -> Result<HoldingsResponse, AppError> {
    // 口座指定がある場合、存在確認を先に済ませる。
    // 存在しない口座を黙って空配列で返すと、typo に気づけない。
    #[allow(clippy::collapsible_if)]
    if let Some(account_id) = query.account_id {
        if !holding_repo::account_exists(db, user_id, account_id).await? {
            return Err(AppError::NotFound("口座"));
        }
    }

    let rows = holding_repo::fetch_trades_for_holdings(db, user_id, query.account_id).await?;
    let prices: BTreeMap<Uuid, LatestPriceRow> = holding_repo::fetch_latest_prices(db, user_id)
        .await?
        .into_iter()
        .map(|p| (p.asset_id, p))
        .collect();

    let mut holdings = Vec::new();

    // rows は (account_id, asset_id, traded_at, created_at, id) 順に並んでいるので、
    // 隣接する同一キーをまとめるだけでポジション単位に分割できる。
    for group in rows.chunk_by(|a, b| a.account_id == b.account_id && a.asset_id == b.asset_id) {
        let head = &group[0];

        // NOTE: Trade のフィールドが private なら Trade::buy / Trade::sell に置き換える。
        let trades: Vec<Trade> = group
            .iter()
            .map(|r: &TradeRow| Trade {
                kind: r.kind,
                quantity: r.quantity,
                price: r.price,
                fee: r.fee,
            })
            .collect();

        // ここでの失敗は取引データの不整合（#8 のハンドラで弾いているはず）＝ 5xx。
        // Internal は anyhow::Error を包むため、詳細はログに出し、応答は trace_id のみになる。
        let holding = build_holding(&trades, head.price_unit).map_err(|e| {
            tracing::error!(
                error = ?e,
                account_id = %head.account_id,
                asset_id = %head.asset_id,
                "取引の畳み込みに失敗した"
            );
            AppError::Internal(anyhow::anyhow!(
                "保有ポジションを算出できませんでした (account_id={}, asset_id={}): {e}",
                head.account_id,
                head.asset_id
            ))
        })?;

        // 全売却済みは既定で除外する。
        if !query.include_closed && holding.quantity == Decimal::ZERO {
            continue;
        }

        let price = prices.get(&head.asset_id);
        let valuation = price.map(|p| evaluate(&holding, p.price, head.price_unit));

        holdings.push(HoldingItem {
            account_id: head.account_id,
            account_name: head.account_name.clone(),
            account_type: head.account_type,
            asset_id: head.asset_id,
            symbol: head.symbol.clone(),
            name: head.asset_name.clone(),
            asset_class: head.asset_class,
            currency: head.currency.clone(),
            price_unit: head.price_unit,

            quantity: holding.quantity,
            avg_cost: holding.avg_cost,
            book_value: holding.book_value,
            realized_pnl: holding.realized_pnl,

            price: price.map(|p| p.price),
            priced_on: price.map(|p| p.priced_on),
            market_value: valuation.as_ref().map(|v| v.market_value),
            unrealized_pnl: valuation.as_ref().map(|v| v.unrealized_pnl),
            unrealized_pnl_rate: valuation.as_ref().and_then(|v| v.unrealized_pnl_rate),
        });
    }

    // 出力順は口座名 → シンボル。account_id 順のままだと UUID 依存で人間には無意味。
    holdings.sort_by(|a, b| {
        a.account_name
            .cmp(&b.account_name)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });

    let summary = summarize(&holdings);

    Ok(HoldingsResponse { holdings, summary })
}

/// 通貨ごとの累算器。`Totals` に確定させる前の途中状態。
#[derive(Debug, Default)]
struct Accumulator {
    book_value: Decimal,
    /// 価格があった銘柄の簿価。騰落率の分母になる。
    priced_book_value: Decimal,
    market_value: Decimal,
    unrealized_pnl: Decimal,
    realized_pnl: Decimal,
    unpriced_count: usize,
}

impl Accumulator {
    fn add(&mut self, item: &HoldingItem) {
        self.book_value += item.book_value;
        self.realized_pnl += item.realized_pnl;

        match (item.market_value, item.unrealized_pnl) {
            (Some(market_value), Some(unrealized_pnl)) => {
                self.priced_book_value += item.book_value;
                self.market_value += market_value;
                self.unrealized_pnl += unrealized_pnl;
            }
            _ => self.unpriced_count += 1,
        }
    }

    fn finish(self, currency: String) -> Totals {
        let unrealized_pnl_rate = if self.priced_book_value == Decimal::ZERO {
            None
        } else {
            Some(self.unrealized_pnl / self.priced_book_value)
        };

        Totals {
            currency,
            book_value: self.book_value,
            market_value: self.market_value,
            unrealized_pnl: self.unrealized_pnl,
            unrealized_pnl_rate,
            realized_pnl: self.realized_pnl,
            unpriced_count: self.unpriced_count,
        }
    }
}

fn totals_by_currency<'a>(items: impl Iterator<Item = &'a HoldingItem>) -> Vec<Totals> {
    let mut acc: BTreeMap<String, Accumulator> = BTreeMap::new();

    for item in items {
        acc.entry(item.currency.clone()).or_default().add(item);
    }

    acc.into_iter()
        .map(|(currency, a)| a.finish(currency))
        .collect()
}

fn summarize(holdings: &[HoldingItem]) -> HoldingsSummary {
    let totals = totals_by_currency(holdings.iter());
    let unpriced_count = holdings.iter().filter(|h| h.price.is_none()).count();

    // 口座ごとの内訳。holdings は口座名順に並んでいるので、その順序をそのまま使う。
    let mut by_account: Vec<AccountSummary> = Vec::new();
    for group in holdings.chunk_by(|a, b| a.account_id == b.account_id) {
        let head = &group[0];
        by_account.push(AccountSummary {
            account_id: head.account_id,
            account_name: head.account_name.clone(),
            account_type: head.account_type,
            totals: totals_by_currency(group.iter()),
        });
    }

    HoldingsSummary {
        unpriced_count,
        totals,
        by_account,
    }
}
