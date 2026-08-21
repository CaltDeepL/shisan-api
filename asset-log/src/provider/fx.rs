use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::domain::currency::Currency;

/// 為替レート1件。`rated_on` は ECB が公表した日で、要求した日付とは限らない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxRate {
    pub base: Currency,
    pub quote: Currency,
    /// 実際にレートが成立した日（週末を要求すれば直近営業日が入る）
    pub rated_on: NaiveDate,
    pub rate: Decimal,
    /// 外部APIに到達できず、キャッシュ済みの過去値でしのいだ場合に true
    pub is_stale: bool,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum FxError {
    #[error("unsupported currency pair: {base}/{quote}")]
    UnsupportedPair { base: Currency, quote: Currency },
    #[error("no usable rate for {base}/{quote} as of {on}")]
    Unavailable {
        base: Currency,
        quote: Currency,
        on: NaiveDate,
    },
    /// 一時障害（タイムアウト・5xx）。デコレータがフォールバックを試みる対象
    #[error("fx upstream transient failure: {0}")]
    Transient(String),
    /// 恒久的な失敗（不正JSON・想定外の応答）。リトライしない
    #[error("fx upstream failure: {0}")]
    Upstream(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl FxError {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

#[async_trait]
pub trait FxRateProvider: Send + Sync {
    async fn rate(&self, base: Currency, quote: Currency, on: NaiveDate)
    -> Result<FxRate, FxError>;
}

// ---------------------------------------------------------------- Frankfurter

pub struct FrankfurterClient {
    http: reqwest::Client,
    base_url: String,
    max_attempts: u32,
}

#[derive(serde::Deserialize)]
struct FrankfurterResponse {
    date: NaiveDate,
    #[allow(dead_code)]
    base: String,
    rates: HashMap<String, Decimal>,
}

impl FrankfurterClient {
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            max_attempts: 3,
        })
    }

    fn url(&self, on: NaiveDate, base: Currency, quote: Currency) -> String {
        format!(
            "{}/{}?base={}&symbols={}",
            self.base_url,
            on.format("%Y-%m-%d"),
            base,
            quote
        )
    }
}

#[async_trait]
impl FxRateProvider for FrankfurterClient {
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
                fetched_at: chrono::Utc::now(),
            });
        }

        let url = self.url(on, base, quote);
        let mut last: FxError = FxError::Transient("not attempted".into());

        for attempt in 0..self.max_attempts {
            if attempt > 0 {
                // 指数バックオフ: 200ms, 400ms
                let wait = Duration::from_millis(200 * (1 << (attempt - 1)));
                tokio::time::sleep(wait).await;
            }

            match self.http.get(&url).send().await {
                Err(e) => {
                    // タイムアウト・接続断はリトライ対象
                    last = FxError::Transient(e.to_string());
                }
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_server_error() {
                        last = FxError::Transient(format!("upstream returned {status}"));
                        continue;
                    }
                    if status == reqwest::StatusCode::NOT_FOUND
                        || status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
                    {
                        // 通貨コードが Frankfurter の対応外。リトライしても無駄
                        return Err(FxError::UnsupportedPair { base, quote });
                    }
                    if !status.is_success() {
                        return Err(FxError::Upstream(format!("upstream returned {status}")));
                    }

                    let body: FrankfurterResponse = resp
                        .json()
                        .await
                        .map_err(|e| FxError::Upstream(format!("invalid response body: {e}")))?;

                    let rate = body
                        .rates
                        .get(quote.as_str())
                        .copied()
                        .ok_or(FxError::UnsupportedPair { base, quote })?;

                    if rate <= Decimal::ZERO {
                        return Err(FxError::Upstream(format!("non-positive rate: {rate}")));
                    }

                    return Ok(FxRate {
                        base,
                        quote,
                        // 要求日ではなく、応答が示す実日付を採用する
                        rated_on: body.date,
                        rate,
                        is_stale: false,
                        fetched_at: chrono::Utc::now(),
                    });
                }
            }
        }

        Err(last)
    }
}
