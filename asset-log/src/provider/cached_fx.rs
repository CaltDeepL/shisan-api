use async_trait::async_trait;
use chrono::{Datelike, NaiveDate, Utc, Weekday};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::{
    domain::currency::Currency,
    provider::fx::{FxError, FxRate, FxRateProvider},
    repository::fx_repo,
};

/// 何日前までのレートを「使ってよい」とみなすかの方針。
#[derive(Debug, Clone, Copy)]
pub struct StalePolicy {
    pub max_calendar_days: i64,
    pub max_business_days: i64,
}

impl Default for StalePolicy {
    fn default() -> Self {
        // 平日なら実質24〜48時間、週末・祝日を挟めば最大96時間
        Self {
            max_calendar_days: 4,
            max_business_days: 2,
        }
    }
}

impl StalePolicy {
    pub fn accepts(&self, rated_on: NaiveDate, wanted: NaiveDate) -> bool {
        if rated_on > wanted {
            return false;
        }
        let calendar = (wanted - rated_on).num_days();
        calendar <= self.max_calendar_days
            && business_days_between(rated_on, wanted) <= self.max_business_days
    }
}

/// `from` の翌日から `to` までに含まれる平日数。土日のみ除外する（祝日は考慮しない）。
fn business_days_between(from: NaiveDate, to: NaiveDate) -> i64 {
    from.iter_days()
        .skip(1)
        .take_while(|d| *d <= to)
        .filter(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
        .count() as i64
}

pub struct CachedFxProvider<P> {
    inner: P,
    pool: PgPool,
    policy: StalePolicy,
}

impl<P: FxRateProvider> CachedFxProvider<P> {
    pub fn new(inner: P, pool: PgPool, policy: StalePolicy) -> Self {
        Self {
            inner,
            pool,
            policy,
        }
    }

    async fn cached(
        &self,
        base: Currency,
        quote: Currency,
        on: NaiveDate,
    ) -> Result<Option<fx_repo::CachedRate>, FxError> {
        Ok(fx_repo::find_latest_on_or_before(&self.pool, base, quote, on).await?)
    }
}

#[async_trait]
impl<P: FxRateProvider> FxRateProvider for CachedFxProvider<P> {
    async fn rate(
        &self,
        base: Currency,
        quote: Currency,
        on: NaiveDate,
    ) -> Result<FxRate, FxError> {
        if base == quote {
            return Ok(FxRate {
                base,
                quote,
                rated_on: on,
                rate: Decimal::ONE,
                is_stale: false,
                fetched_at: Utc::now(),
            });
        }

        let hit = self.cached(base, quote, on).await?;
        let is_historical = on < Utc::now().date_naive();

        // 過去日のECB公表値は不変なので、方針の範囲内なら外部に問い合わせない。
        // 当日分は未公表の可能性があるため、完全一致以外は取りに行く。
        if let Some(c) = &hit {
            let usable = c.rated_on == on || (is_historical && self.policy.accepts(c.rated_on, on));
            if usable {
                return Ok(FxRate {
                    base,
                    quote,
                    rated_on: c.rated_on,
                    rate: c.rate,
                    is_stale: false,
                    fetched_at: c.fetched_at,
                });
            }
        }

        match self.inner.rate(base, quote, on).await {
            Ok(fresh) => {
                fx_repo::upsert(&self.pool, base, quote, fresh.rated_on, fresh.rate).await?;
                Ok(fresh)
            }
            Err(e) if e.is_transient() => {
                tracing::warn!(
                    error = %e, base = %base, quote = %quote, on = %on,
                    "fx upstream unavailable; falling back to cache"
                );
                match hit {
                    Some(c) if self.policy.accepts(c.rated_on, on) => Ok(FxRate {
                        base,
                        quote,
                        rated_on: c.rated_on,
                        rate: c.rate,
                        is_stale: true,
                        fetched_at: c.fetched_at,
                    }),
                    _ => Err(FxError::Unavailable { base, quote, on }),
                }
            }
            Err(e) => Err(e),
        }
    }
}
