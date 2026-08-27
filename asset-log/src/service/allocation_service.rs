use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::provider::fx::{FxError, FxRateProvider};
use crate::service::analytics_service::{self, Granularity, GroupBy};

/// 分類軸で集約したあと、比率を付ける前の1項目
#[derive(Debug, Clone)]
pub struct Slice {
    pub key: String,
    pub label: String,
    pub value_jpy: Decimal,
}

/// レスポンスの1項目
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AllocationItem {
    pub key: String,
    pub label: String,
    #[schema(value_type = String, example = "500000")]
    pub value_jpy: Decimal,
    /// 構成比（パーセント）。合計は必ず 100.00 になる
    #[schema(value_type = String, example = "33.34")]
    pub ratio: Decimal,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AllocationResult {
    pub as_of: NaiveDate,
    /// 常に "JPY"
    pub base_currency: String,
    pub group_by: GroupBy,
    /// 売買のある銘柄の評価額のみが対象。現金・預金残高は含まない。
    #[schema(example = "securities_only")]
    pub scope: &'static str,
    pub fx_stale: bool,
    #[schema(value_type = String)]
    pub total_value_jpy: Decimal,
    pub unpriced_asset_count: i64,
    pub items: Vec<AllocationItem>,
}

/// 評価額の構成比を 0.01 刻みで配分する。
///
/// 戻り値の `ratio` の合計は、項目が1件以上あれば必ず `100.00` になる。
/// 単純な四捨五入では 99.99 / 100.01 に振れるため、
/// 切り捨て → 端数の大きい順に 0.01 ずつ配り直す（最大剰余法）。
pub fn assign_ratios(slices: Vec<Slice>) -> Vec<AllocationItem> {
    /// 100.00% を 0.01 刻みで表したときの総単位数
    const TOTAL_UNITS: i64 = 10_000;

    let mut slices: Vec<Slice> = slices
        .into_iter()
        .filter(|s| s.value_jpy > Decimal::ZERO)
        .collect();

    if slices.is_empty() {
        return Vec::new();
    }

    // 出力順をここで確定させる。以降の安定ソートがこの順序をタイブレークに使う
    slices.sort_by(|a, b| {
        b.value_jpy
            .cmp(&a.value_jpy)
            .then_with(|| a.key.cmp(&b.key))
    });

    let total: Decimal = slices.iter().map(|s| s.value_jpy).sum();
    let units = Decimal::from(TOTAL_UNITS);

    let mut floors: Vec<i64> = Vec::with_capacity(slices.len());
    let mut remainders: Vec<(usize, Decimal)> = Vec::with_capacity(slices.len());

    for (idx, s) in slices.iter().enumerate() {
        let raw = s.value_jpy * units / total;
        let floor = raw.floor();
        floors.push(floor.to_i64().unwrap_or(0));
        remainders.push((idx, raw - floor));
    }

    // 端数の降順。sort_by は安定ソートなので、同値なら上で決めた順序が残る
    remainders.sort_by_key(|b| std::cmp::Reverse(b.1));
    // 各項目が失う端数は 1 単位未満なので、不足分は必ず項目数未満に収まる
    let deficit = (TOTAL_UNITS - floors.iter().sum::<i64>()).max(0) as usize;
    for (idx, _) in remainders.iter().take(deficit) {
        floors[*idx] += 1;
    }

    slices
        .into_iter()
        .zip(floors)
        .map(|(s, u)| AllocationItem {
            key: s.key,
            label: s.label,
            value_jpy: s.value_jpy,
            ratio: Decimal::new(u, 2),
        })
        .collect()
}

pub async fn allocation(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    user_id: Uuid,
    as_of: NaiveDate,
    group_by: GroupBy,
) -> Result<AllocationResult, FxError> {
    // 1日だけの時系列として評価する。#11 と同じ経路を通すことで、
    // 折れ線グラフの合計と円グラフの合計が構造的に一致する。
    let history =
        analytics_service::asset_history(db, fx, user_id, as_of, as_of, Granularity::Day, group_by)
            .await?;

    let mut slices: Vec<Slice> = Vec::with_capacity(history.series.len());
    let mut unpriced_asset_count = 0i64;

    for s in history.series {
        // 1日分しか要求していないので点は高々1つ。念のため最後の点を採る
        let Some(p) = s.points.into_iter().next_back() else {
            continue;
        };
        unpriced_asset_count += p.unpriced_asset_count;
        slices.push(Slice {
            key: s.key,
            label: s.label,
            value_jpy: p.market_value_jpy,
        });
    }

    let total_value_jpy: Decimal = slices.iter().map(|s| s.value_jpy).sum();

    Ok(AllocationResult {
        as_of,
        base_currency: history.base_currency,
        group_by,
        scope: "securities_only",
        fx_stale: history.fx_stale,
        total_value_jpy,
        unpriced_asset_count,
        items: assign_ratios(slices),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(key: &str, value: i64) -> Slice {
        Slice {
            key: key.to_string(),
            label: key.to_string(),
            value_jpy: Decimal::from(value),
        }
    }

    fn sum_ratio(items: &[AllocationItem]) -> Decimal {
        items.iter().map(|i| i.ratio).sum()
    }

    #[test]
    fn exact_division_keeps_ratios() {
        let items = assign_ratios(vec![
            slice("a", 500_000),
            slice("b", 300_000),
            slice("c", 200_000),
        ]);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].ratio, Decimal::new(5000, 2));
        assert_eq!(items[1].ratio, Decimal::new(3000, 2));
        assert_eq!(items[2].ratio, Decimal::new(2000, 2));
        assert_eq!(sum_ratio(&items), Decimal::new(10000, 2));
    }

    #[test]
    fn three_way_split_still_sums_to_100() {
        let items = assign_ratios(vec![
            slice("a", 1_000_000),
            slice("b", 1_000_000),
            slice("c", 1_000_000),
        ]);
        assert_eq!(items[0].ratio, Decimal::new(3334, 2));
        assert_eq!(items[1].ratio, Decimal::new(3333, 2));
        assert_eq!(items[2].ratio, Decimal::new(3333, 2));
        assert_eq!(sum_ratio(&items), Decimal::new(10000, 2));
    }

    #[test]
    fn single_slice_is_100() {
        let items = assign_ratios(vec![slice("a", 123_456)]);
        assert_eq!(items[0].ratio, Decimal::new(10000, 2));
    }

    #[test]
    fn scattered_remainders_sum_to_100() {
        let values = [1, 3, 5, 7, 11, 13, 17];
        let slices = values
            .iter()
            .enumerate()
            .map(|(i, v)| slice(&format!("k{i}"), *v))
            .collect();
        let items = assign_ratios(slices);
        assert_eq!(items.len(), 7);
        assert_eq!(sum_ratio(&items), Decimal::new(10000, 2));
    }

    #[test]
    fn zero_value_slice_is_excluded() {
        let items = assign_ratios(vec![slice("a", 100_000), slice("zero", 0)]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "a");
        assert_eq!(sum_ratio(&items), Decimal::new(10000, 2));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(assign_ratios(vec![]).is_empty());
        assert!(assign_ratios(vec![slice("a", 0)]).is_empty());
    }

    #[test]
    fn slices_are_sorted_by_value_desc() {
        let items = assign_ratios(vec![
            slice("small", 100),
            slice("big", 900),
            slice("mid", 500),
        ]);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec!["big", "mid", "small"]);
    }
}
