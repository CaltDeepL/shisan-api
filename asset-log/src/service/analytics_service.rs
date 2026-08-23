use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::AccountType;
use crate::domain::asset::AssetClass;
use crate::domain::currency::Currency;
use crate::domain::position::{Holding, Trade, apply_trade};
use crate::provider::fx::{FxError, FxRateProvider};
use crate::repository::analytics_repo::{self, HistoryTrade};
use crate::service::fx_history;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    Day,
    Month,
}

impl Granularity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    None,
    AccountType,
    AssetClass,
    Account,
    Asset,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryPoint {
    pub date: NaiveDate,
    pub market_value_jpy: Decimal,
    pub cost_jpy: Decimal,
    /// その日、価格または約定日レートが引けず評価から外した銘柄数
    pub unpriced_asset_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistorySeries {
    pub key: String,
    /// 画面表示用の名前。enum 軸では日本語名、口座・銘柄軸では登録名。
    pub label: String,
    pub points: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryResult {
    pub granularity: Granularity,
    pub base_currency: String,
    /// 為替の補充中に外部APIへ到達できず、キャッシュでしのいだ
    pub fx_stale: bool,
    pub series: Vec<HistorySeries>,
}

/// ポジション単位の畳み込み結果。取引日ごとの `Holding` を昇順で持つ。
struct PositionTimeline {
    asset_id: Uuid,
    asset_class: AssetClass,
    group_key: String,
    group_label: String,
    price_unit: Decimal,
    currency: Currency,
    /// 約定日レートが1件でも引けなかった場合 true。評価から外す。
    unconvertible: bool,
    /// (取引日, その日の終わりの保有状態)
    snapshots: Vec<(NaiveDate, Holding)>,
}

impl PositionTimeline {
    /// `on` 時点の保有状態。取引開始前なら None。
    fn at(&self, on: NaiveDate) -> Option<&Holding> {
        let i = self.snapshots.partition_point(|(d, _)| *d <= on);
        if i == 0 {
            None
        } else {
            Some(&self.snapshots[i - 1].1)
        }
    }
}

/// 日付昇順のレート列から、`on` 以前で最も新しいレートを引く。
fn rate_on(points: &[(NaiveDate, Decimal)], on: NaiveDate) -> Option<Decimal> {
    let i = points.partition_point(|(d, _)| *d <= on);
    if i == 0 { None } else { Some(points[i - 1].1) }
}

fn serde_key<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => "unknown".to_owned(),
    }
}

/// 分類軸に応じた (キー, 表示名) を返す。
fn group_of(t: &HistoryTrade, by: GroupBy) -> (String, String) {
    match by {
        GroupBy::None => ("total".to_owned(), "合計".to_owned()),
        GroupBy::AccountType => {
            let key = serde_key(&t.account_type);
            let label = account_type_label(t.account_type).to_owned();
            (key, label)
        }
        GroupBy::AssetClass => {
            let key = serde_key(&t.asset_class);
            let label = asset_class_label(t.asset_class).to_owned();
            (key, label)
        }
        GroupBy::Account => (t.account_id.to_string(), t.account_name.clone()),
        GroupBy::Asset => (
            t.asset_id.to_string(),
            format!("{} {}", t.symbol, t.asset_name),
        ),
    }
}

fn account_type_label(v: AccountType) -> &'static str {
    match v {
        AccountType::Tokutei => "特定口座",
        AccountType::Ippan => "一般口座",
        AccountType::NisaTsumitate => "NISA（つみたて投資枠）",
        AccountType::NisaGrowth => "NISA（成長投資枠）",
        AccountType::Ideco => "iDeCo",
        AccountType::Bank => "銀行口座",
    }
}

fn asset_class_label(v: AssetClass) -> &'static str {
    match v {
        AssetClass::Equity => "株式",
        AssetClass::Etf => "ETF",
        AssetClass::MutualFund => "投資信託",
        AssetClass::Bond => "債券",
        AssetClass::Cash => "現金",
        AssetClass::Other => "その他",
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn asset_history(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    granularity: Granularity,
    group_by: GroupBy,
) -> Result<HistoryResult, FxError> {
    let jpy: Currency = "JPY".parse().expect("JPY is valid");

    let trades = analytics_repo::fetch_trades_until(db, user_id, to).await?;
    let grid =
        analytics_repo::fetch_price_grid(db, user_id, from, to, granularity.as_str()).await?;

    // 日付スピン。generate_series が作った日付をそのまま使う
    let mut dates: Vec<NaiveDate> = grid.iter().map(|g| g.on_date).collect();
    dates.sort_unstable();
    dates.dedup();

    if trades.is_empty() || dates.is_empty() {
        return Ok(HistoryResult {
            granularity,
            base_currency: jpy.to_string(),
            fx_stale: false,
            series: Vec::new(),
        });
    }

    let mut price_of: HashMap<(NaiveDate, Uuid), Decimal> = HashMap::new();
    for g in &grid {
        if let Some(p) = g.price {
            price_of.insert((g.on_date, g.asset_id), p);
        }
    }

    // --- 為替 -------------------------------------------------------------
    // 簿価は取得時レートで換算するため、`from` ではなく最古の取引日まで遡る
    let earliest = trades.iter().map(|t| t.traded_at).min().expect("non-empty");
    let fx_from = earliest.min(from);

    let mut fx_stale = false;
    let mut fx_of: HashMap<Currency, Vec<(NaiveDate, Decimal)>> = HashMap::new();
    let mut currencies: HashSet<Currency> = HashSet::new();
    for t in &trades {
        let c: Currency = t
            .currency
            .parse()
            .map_err(|_| FxError::Upstream(format!("invalid currency in db: {}", t.currency)))?;
        currencies.insert(c);
    }
    for c in currencies {
        if c == jpy {
            continue;
        }
        let s = fx_history::load(db, fx, c, jpy, fx_from, to).await?;
        fx_stale |= s.is_stale;
        fx_of.insert(c, s.points);
    }

    // --- ポジションごとの畳み込み -----------------------------------------
    let mut timelines: HashMap<(Uuid, Uuid), PositionTimeline> = HashMap::new();

    for t in &trades {
        let currency: Currency = t.currency.parse().expect("validated above");

        // 約定日のレートで price / fee をJPYに寄せてから畳み込む。
        // これにより Holding.book_value が最初からJPY建てになる。
        let fx_rate = if currency == jpy {
            Some(Decimal::ONE)
        } else {
            fx_of.get(&currency).and_then(|p| rate_on(p, t.traded_at))
        };

        let (gkey, glabel) = group_of(t, group_by);
        let tl = timelines
            .entry((t.account_id, t.asset_id))
            .or_insert_with(|| PositionTimeline {
                asset_id: t.asset_id,
                asset_class: t.asset_class,
                group_key: gkey,
                group_label: glabel,
                price_unit: t.price_unit,
                currency,
                unconvertible: false,
                snapshots: Vec::new(),
            });

        let Some(rate) = fx_rate else {
            tl.unconvertible = true;
            continue;
        };

        let jpy_trade = Trade {
            kind: t.kind,
            quantity: t.quantity,
            price: t.price * rate,
            fee: t.fee * rate,
        };

        let mut state = tl
            .snapshots
            .last()
            .map(|(_, h)| h.clone())
            .unwrap_or_default();

        // DB側で ORDER BY 済みなので、Oversell は起きない想定。
        // 起きた場合はデータ不整合なので、そのポジションを評価対象から外す。
        if apply_trade(&mut state, &jpy_trade, tl.price_unit).is_err() {
            tl.unconvertible = true;
            continue;
        }

        match tl.snapshots.last_mut() {
            Some((d, h)) if *d == t.traded_at => *h = state,
            _ => tl.snapshots.push((t.traded_at, state)),
        }
    }

    // --- 日付 × ポジション を合算 -----------------------------------------
    struct Acc {
        market: Decimal,
        cost: Decimal,
        unpriced: HashSet<Uuid>,
    }

    // (キー, 表示名) の一覧。キーで一意化する
    let mut keyed: Vec<(String, String)> = timelines
        .values()
        .map(|t| (t.group_key.clone(), t.group_label.clone()))
        .collect();
    keyed.sort();
    keyed.dedup_by(|a, b| a.0 == b.0);
    let keys: Vec<String> = keyed.iter().map(|(k, _)| k.clone()).collect();

    let mut acc: HashMap<(String, NaiveDate), Acc> = HashMap::new();
    for k in &keys {
        for d in &dates {
            acc.insert(
                (k.clone(), *d),
                Acc {
                    market: Decimal::ZERO,
                    cost: Decimal::ZERO,
                    unpriced: HashSet::new(),
                },
            );
        }
    }

    for tl in timelines.values() {
        for d in &dates {
            let Some(h) = tl.at(*d) else { continue };
            if h.quantity.is_zero() {
                continue; // 全売却済み。点は打つが寄与ゼロ
            }
            let a = acc
                .get_mut(&(tl.group_key.clone(), *d))
                .expect("preallocated");

            if tl.unconvertible {
                a.unpriced.insert(tl.asset_id);
                continue;
            }
            // 現金は額面評価。価格未登録を理由に外さない
            let fx_today = if tl.currency == jpy {
                Some(Decimal::ONE)
            } else {
                fx_of.get(&tl.currency).and_then(|p| rate_on(p, *d))
            };
            let Some(rate) = fx_today else {
                a.unpriced.insert(tl.asset_id);
                continue;
            };

            // 現金は額面評価。価格テーブルを引かない
            if !tl.asset_class.is_priceable() {
                a.market += h.quantity * rate;
                a.cost += h.book_value;
                continue;
            }

            let Some(price) = price_of.get(&(*d, tl.asset_id)).copied() else {
                a.unpriced.insert(tl.asset_id);
                continue;
            };

            a.market += h.quantity * price * rate / tl.price_unit;
            a.cost += h.book_value; // 取得時レートで換算済み
        }
    }

    let series = keyed
        .into_iter()
        .map(|(key, label)| {
            let points = dates
                .iter()
                .map(|d| {
                    let a = &acc[&(key.clone(), *d)];
                    HistoryPoint {
                        date: *d,
                        market_value_jpy: a.market.round_dp(0),
                        cost_jpy: a.cost.round_dp(0),
                        unpriced_asset_count: a.unpriced.len() as i64,
                    }
                })
                .collect();
            HistorySeries { key, label, points }
        })
        .collect();
    Ok(HistoryResult {
        granularity,
        base_currency: jpy.to_string(),
        fx_stale,
        series,
    })
}
