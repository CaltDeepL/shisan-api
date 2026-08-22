use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::domain::currency::Currency;
use crate::provider::fx::{FxError, FxRateProvider};
use crate::repository::fx_repo;

/// ECB最長連休（イースター金〜月）でも連続レート間は5日。7日なら誤検出しない。
const MAX_GAP_DAYS: i32 = 7;
/// 起点を引くための遡り幅。年末年始を跨いでも1件は拾える。
const SEED_LOOKBACK_DAYS: i64 = 30;

pub struct FxSeries {
    /// 日付昇順。ECB休場日は含まれないので、利用側が前値を引き継ぐ。
    pub points: Vec<(NaiveDate, Decimal)>,
    /// 補充が必要だったが外部APIに到達できず、キャッシュでしのいだ
    pub is_stale: bool,
}
/// 末尾の鮮度判定。StalePolicy の既定値と揃える。
const MAX_TAIL_DAYS: i64 = 4;

pub async fn load(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    base: Currency,
    quote: Currency,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<FxSeries, FxError> {
    let cov = fx_repo::coverage(db, base, quote, from, to).await?;

    // 期間内が空でも、直前のシードが十分新しければ有効（週末だけの期間など）
    let effective_newest = cov.newest_on.or(cov.seed_on);
    let stale_tail = match effective_newest {
        None => true,
        Some(n) => n < to - Duration::days(MAX_TAIL_DAYS),
    };
    let needs_backfill =
        cov.seed_on.is_none() || cov.max_gap.is_some_and(|g| g > MAX_GAP_DAYS) || stale_tail;

    let mut is_stale = false;
    if needs_backfill {
        let fetch_from = from - Duration::days(SEED_LOOKBACK_DAYS);
        match fx.rates_in_range(base, quote, fetch_from, to).await {
            Ok(points) => {
                let rows: Vec<(NaiveDate, Decimal)> =
                    points.into_iter().map(|p| (p.rated_on, p.rate)).collect();
                fx_repo::upsert_many(db, base, quote, &rows).await?;
            }
            // 一時障害はキャッシュに縮退。恒久的な失敗は伝播させる
            Err(e) if e.is_transient() => is_stale = true,
            Err(e) => return Err(e),
        }
    }

    let cached = fx_repo::find_in_range(
        db,
        base,
        quote,
        from - Duration::days(SEED_LOOKBACK_DAYS),
        to,
    )
    .await?;

    Ok(FxSeries {
        points: cached.into_iter().map(|c| (c.rated_on, c.rate)).collect(),
        is_stale,
    })
}
