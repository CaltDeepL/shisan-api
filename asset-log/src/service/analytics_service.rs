use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::account::AccountType;
use crate::domain::asset::AssetClass;
use crate::domain::currency::Currency;
use crate::domain::position::{Holding, Trade, apply_trade};
use crate::provider::fx::{FxError, FxRateProvider};
use crate::repository::analytics_repo::{self, HistoryTrade};
use crate::repository::snapshot_repo::{self, SnapshotWithMeta};
use crate::service::fx_history;
use utoipa::ToSchema;
/// asset_history の評価結果がどちらの経路から来たか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistorySource {
    /// 取引履歴から都度再計算した
    Computed,
    /// 日次スナップショットから読み出した
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    None,
    AccountType,
    AssetClass,
    Account,
    Asset,
}

#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct HistoryPoint {
    pub date: NaiveDate,
    #[schema(value_type = String, example = "1234567")]
    pub market_value_jpy: Decimal,
    #[schema(value_type = String, example = "1000000")]
    pub cost_jpy: Decimal,
    /// その日、価格または約定日レートが引けず評価から外した銘柄数
    pub unpriced_asset_count: i64,
}

#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct HistorySeries {
    /// 分類軸のキー。group_by=none なら "total"、口座・銘柄軸ではUUID
    pub key: String,
    /// 画面表示用の名前。enum 軸では日本語名、口座・銘柄軸では登録名。
    pub label: String,
    pub points: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct HistoryResult {
    pub granularity: Granularity,
    /// 常に "JPY"
    pub base_currency: String,
    /// 為替の補充中に外部APIへ到達できず、キャッシュでしのいだ
    pub fx_stale: bool,
    pub series: Vec<HistorySeries>,
    pub source: HistorySource,
}

/// ポジション単位の畳み込み結果。取引日ごとの `Holding` を昇順で持つ。
/// 分類軸（group_by）には依存しない。
pub(crate) struct PositionTimeline {
    pub(crate) account_id: Uuid,
    pub(crate) asset_id: Uuid,
    pub(crate) asset_class: AssetClass,
    pub(crate) price_unit: Decimal,
    pub(crate) currency: Currency,
    /// 約定日レートが1件でも引けなかった場合 true。評価から外す。
    pub(crate) unconvertible: bool,
    /// (取引日, その日の終わりの保有状態)
    pub(crate) snapshots: Vec<(NaiveDate, Holding)>,
}
impl PositionTimeline {
    /// `on` 時点の保有状態。取引開始前なら None。
    pub(crate) fn at(&self, on: NaiveDate) -> Option<&Holding> {
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

/// ある日の、あるポジションの評価結果。グルーピング前の中間表現。
/// asset-history とスナップショット生成が共有する。
pub(crate) struct PositionValue {
    pub account_id: Uuid,
    pub asset_id: Uuid,
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    #[allow(dead_code)]
    pub currency: Currency,
    /// 約定日レートで換算済みのJPY簿価
    pub cost_basis_jpy: Decimal,
    /// 評価額。価格または為替が引けなければ None
    pub market_value_jpy: Option<Decimal>,
    pub price: Option<Decimal>,
    pub fx_rate: Option<Decimal>,
}

/// `on` 時点の全ポジションを評価する。数量ゼロのポジションは含めない。
pub(crate) fn evaluate_day(
    timelines: &HashMap<(Uuid, Uuid), PositionTimeline>,
    price_of: &HashMap<(NaiveDate, Uuid), Decimal>,
    fx_of: &HashMap<Currency, Vec<(NaiveDate, Decimal)>>,
    jpy: Currency,
    on: NaiveDate,
) -> Vec<PositionValue> {
    let mut out = Vec::new();

    for tl in timelines.values() {
        let Some(h) = tl.at(on) else { continue };
        if h.quantity.is_zero() {
            continue;
        }

        let mut v = PositionValue {
            account_id: tl.account_id,
            asset_id: tl.asset_id,
            quantity: h.quantity,
            avg_cost: h.avg_cost,
            currency: tl.currency,
            cost_basis_jpy: h.book_value,
            market_value_jpy: None,
            price: None,
            fx_rate: None,
        };

        if tl.unconvertible {
            out.push(v);
            continue;
        }

        let fx_today = if tl.currency == jpy {
            Some(Decimal::ONE)
        } else {
            fx_of.get(&tl.currency).and_then(|p| rate_on(p, on))
        };
        let Some(rate) = fx_today else {
            out.push(v);
            continue;
        };
        v.fx_rate = Some(rate);

        if !tl.asset_class.is_priceable() {
            v.market_value_jpy = Some(h.quantity * rate);
            out.push(v);
            continue;
        }

        let Some(price) = price_of.get(&(on, tl.asset_id)).copied() else {
            out.push(v);
            continue;
        };
        v.price = Some(price);
        v.market_value_jpy = Some(h.quantity * price * rate / tl.price_unit);
        out.push(v);
    }

    out
}

pub(crate) fn evaluate_context_day(ctx: &EvaluationContext, on: NaiveDate) -> Vec<PositionValue> {
    let jpy: Currency = "JPY".parse().expect("JPY is valid");
    evaluate_day(&ctx.timelines, &ctx.price_of, &ctx.fx_of, jpy, on)
}

/// asset-history と日次スナップショットが共有する評価準備情報。
pub(crate) struct EvaluationContext {
    pub(crate) trades: Vec<HistoryTrade>,
    pub(crate) timelines: HashMap<(Uuid, Uuid), PositionTimeline>,
    pub(crate) price_of: HashMap<(NaiveDate, Uuid), Decimal>,
    pub(crate) fx_of: HashMap<Currency, Vec<(NaiveDate, Decimal)>>,
    pub(crate) dates: Vec<NaiveDate>,
    pub(crate) fx_stale: bool,
}

/// 取引・価格・為替を読み込み、評価の準備を整える。
pub(crate) async fn prepare(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    granularity: Granularity,
) -> Result<EvaluationContext, FxError> {
    let trades = analytics_repo::fetch_trades_until(db, user_id, to).await?;
    let grid =
        analytics_repo::fetch_price_grid(db, user_id, from, to, granularity.as_str()).await?;

    let mut dates: Vec<NaiveDate> = grid.iter().map(|point| point.on_date).collect();
    dates.sort_unstable();
    dates.dedup();

    let mut price_of = HashMap::new();
    for point in &grid {
        if let Some(price) = point.price {
            price_of.insert((point.on_date, point.asset_id), price);
        }
    }

    if trades.is_empty() || dates.is_empty() {
        return Ok(EvaluationContext {
            trades,
            timelines: HashMap::new(),
            price_of,
            fx_of: HashMap::new(),
            dates,
            fx_stale: false,
        });
    }

    let jpy: Currency = "JPY".parse().expect("JPY is valid");
    let earliest = trades
        .iter()
        .map(|trade| trade.traded_at)
        .min()
        .expect("non-empty");
    let fx_from = earliest.min(from);
    let mut fx_stale = false;
    let mut fx_of = HashMap::new();
    let mut currencies = HashSet::new();
    for trade in &trades {
        let currency: Currency = trade.currency.parse().map_err(|_| {
            FxError::Upstream(format!("invalid currency in db: {}", trade.currency))
        })?;
        currencies.insert(currency);
    }
    for currency in currencies {
        if currency == jpy {
            continue;
        }
        let series = fx_history::load(db, fx, currency, jpy, fx_from, to).await?;
        fx_stale |= series.is_stale;
        fx_of.insert(currency, series.points);
    }

    let timelines = fold_positions(&trades, &fx_of, jpy);
    Ok(EvaluationContext {
        trades,
        timelines,
        price_of,
        fx_of,
        dates,
        fx_stale,
    })
}

fn serde_key<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => "unknown".to_owned(),
    }
}

/// 分類軸解決に必要な項目。取引履歴・スナップショットどちらの行からも作れる。
struct PositionMeta<'a> {
    account_id: Uuid,
    account_name: &'a str,
    account_type: AccountType,
    asset_id: Uuid,
    symbol: &'a str,
    asset_name: &'a str,
    asset_class: AssetClass,
}

impl<'a> From<&'a HistoryTrade> for PositionMeta<'a> {
    fn from(t: &'a HistoryTrade) -> Self {
        Self {
            account_id: t.account_id,
            account_name: &t.account_name,
            account_type: t.account_type,
            asset_id: t.asset_id,
            symbol: &t.symbol,
            asset_name: &t.asset_name,
            asset_class: t.asset_class,
        }
    }
}

impl<'a> From<&'a SnapshotWithMeta> for PositionMeta<'a> {
    fn from(s: &'a SnapshotWithMeta) -> Self {
        Self {
            account_id: s.account_id,
            account_name: &s.account_name,
            account_type: s.account_type,
            asset_id: s.asset_id,
            symbol: &s.symbol,
            asset_name: &s.asset_name,
            asset_class: s.asset_class,
        }
    }
}

/// 分類軸に応じた (キー, 表示名) を返す。
fn group_of(m: &PositionMeta, by: GroupBy) -> (String, String) {
    match by {
        GroupBy::None => ("total".to_owned(), "合計".to_owned()),
        GroupBy::AccountType => {
            let key = serde_key(&m.account_type);
            let label = account_type_label(m.account_type).to_owned();
            (key, label)
        }
        GroupBy::AssetClass => {
            let key = serde_key(&m.asset_class);
            let label = asset_class_label(m.asset_class).to_owned();
            (key, label)
        }
        GroupBy::Account => (m.account_id.to_string(), m.account_name.to_owned()),
        GroupBy::Asset => (
            m.asset_id.to_string(),
            format!("{} {}", m.symbol, m.asset_name),
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

/// 取引列を (口座, 銘柄) ごとのタイムラインに畳み込む。group_by 非依存。
fn fold_positions(
    trades: &[HistoryTrade],
    fx_of: &HashMap<Currency, Vec<(NaiveDate, Decimal)>>,
    jpy: Currency,
) -> HashMap<(Uuid, Uuid), PositionTimeline> {
    let mut timelines: HashMap<(Uuid, Uuid), PositionTimeline> = HashMap::new();

    for t in trades {
        let currency: Currency = t.currency.parse().expect("validated by caller");
        let fx_rate = if currency == jpy {
            Some(Decimal::ONE)
        } else {
            fx_of.get(&currency).and_then(|p| rate_on(p, t.traded_at))
        };

        let tl = timelines
            .entry((t.account_id, t.asset_id))
            .or_insert_with(|| PositionTimeline {
                account_id: t.account_id,
                asset_id: t.asset_id,
                asset_class: t.asset_class,
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

        if apply_trade(&mut state, &jpy_trade, tl.price_unit).is_err() {
            tl.unconvertible = true;
            continue;
        }

        match tl.snapshots.last_mut() {
            Some((d, h)) if *d == t.traded_at => *h = state,
            _ => tl.snapshots.push((t.traded_at, state)),
        }
    }

    timelines
}

/// 日付ごとの評価結果を系列に畳む。再計算経路とスナップショット経路が共有する。
fn group_and_series(
    per_day: &[(NaiveDate, Vec<PositionValue>)],
    group_of_position: &HashMap<(Uuid, Uuid), (String, String)>,
    dates: &[NaiveDate],
) -> Vec<HistorySeries> {
    struct Acc {
        market: Decimal,
        cost: Decimal,
        unpriced: HashSet<Uuid>,
    }

    // (キー, 表示名) の一覧。キーで一意化する
    let mut keyed: Vec<(String, String)> = group_of_position.values().cloned().collect();
    keyed.sort();
    keyed.dedup_by(|a, b| a.0 == b.0);
    let keys: Vec<String> = keyed.iter().map(|(k, _)| k.clone()).collect();

    let mut acc: HashMap<(String, NaiveDate), Acc> = HashMap::new();
    for k in &keys {
        for d in dates {
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

    for (d, values) in per_day {
        for value in values {
            let (group_key, _) = &group_of_position[&(value.account_id, value.asset_id)];
            let a = acc.get_mut(&(group_key.clone(), *d)).expect("preallocated");
            match value.market_value_jpy {
                Some(market_value) => {
                    a.market += market_value;
                    a.cost += value.cost_basis_jpy;
                }
                None => {
                    a.unpriced.insert(value.asset_id);
                }
            }
        }
    }

    keyed
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
        .collect()
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
    // 対象日を先に確定させる。granularity によって日次か月末かが決まる
    let target_dates =
        analytics_repo::fetch_target_dates(db, from, to, granularity.as_str()).await?;

    if !target_dates.is_empty() {
        let mut conn = db.acquire().await?;
        let covered = snapshot_repo::covered_days(&mut conn, user_id, &target_dates).await?;
        if covered as usize == target_dates.len() {
            return from_snapshots(&mut conn, user_id, &target_dates, granularity, group_by).await;
        }
    }
    // フォールバック: 従来どおり再計算
    compute_history(db, fx, user_id, from, to, granularity, group_by).await
}

/// 取引履歴から都度再計算する経路。スナップショットが揃っていない期間で使う。
async fn compute_history(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    granularity: Granularity,
    group_by: GroupBy,
) -> Result<HistoryResult, FxError> {
    let jpy: Currency = "JPY".parse().expect("JPY is valid");
    let ctx = prepare(db, fx, user_id, from, to, granularity).await?;

    if ctx.trades.is_empty() || ctx.dates.is_empty() {
        return Ok(HistoryResult {
            granularity,
            base_currency: jpy.to_string(),
            fx_stale: false,
            source: HistorySource::Computed,
            series: Vec::new(),
        });
    }

    let mut group_of_position: HashMap<(Uuid, Uuid), (String, String)> = HashMap::new();
    for t in &ctx.trades {
        group_of_position
            .entry((t.account_id, t.asset_id))
            .or_insert_with(|| group_of(&PositionMeta::from(t), group_by));
    }

    let per_day: Vec<(NaiveDate, Vec<PositionValue>)> = ctx
        .dates
        .iter()
        .map(|d| (*d, evaluate_context_day(&ctx, *d)))
        .collect();

    let series = group_and_series(&per_day, &group_of_position, &ctx.dates);

    Ok(HistoryResult {
        granularity,
        base_currency: jpy.to_string(),
        fx_stale: ctx.fx_stale,
        source: HistorySource::Computed,
        series,
    })
}

async fn from_snapshots(
    conn: &mut PgConnection,
    user_id: Uuid,
    dates: &[NaiveDate],
    granularity: Granularity,
    group_by: GroupBy,
) -> Result<HistoryResult, FxError> {
    let rows = snapshot_repo::find_in_range(conn, user_id, dates).await?;

    let mut group_of_position: HashMap<(Uuid, Uuid), (String, String)> = HashMap::new();
    let mut by_day: HashMap<NaiveDate, Vec<PositionValue>> = HashMap::new();

    for r in &rows {
        group_of_position
            .entry((r.account_id, r.asset_id))
            .or_insert_with(|| group_of(&PositionMeta::from(r), group_by));

        by_day
            .entry(r.snapshot_on)
            .or_default()
            .push(PositionValue {
                account_id: r.account_id,
                asset_id: r.asset_id,
                quantity: r.quantity,
                cost_basis_jpy: r.cost_basis_jpy,
                market_value_jpy: r.market_value_jpy,
                // 合算に使わない項目
                avg_cost: Decimal::ZERO,
                currency: "JPY".parse().expect("JPY is valid"),
                price: None,
                fx_rate: None,
            });
    }

    let per_day: Vec<(NaiveDate, Vec<PositionValue>)> = dates
        .iter()
        .map(|d| (*d, by_day.remove(d).unwrap_or_default()))
        .collect();

    let series = group_and_series(&per_day, &group_of_position, dates);

    Ok(HistoryResult {
        granularity,
        base_currency: "JPY".to_owned(),
        // スナップショットは確定値。生成時の為替状況は引き継がない
        fx_stale: false,
        source: HistorySource::Snapshot,
        series,
    })
}
