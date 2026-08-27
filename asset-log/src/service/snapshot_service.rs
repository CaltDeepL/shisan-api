use chrono::{Duration, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::provider::fx::FxRateProvider;
use crate::repository::snapshot_repo::{self, SnapshotRow};
use crate::service::analytics_service;
use utoipa::ToSchema;

#[derive(Debug, Default, serde::Serialize, ToSchema)]
pub struct RunReport {
    pub users: i64,
    pub days: i64,
    pub rows_upserted: i64,
    pub unpriced_rows: i64,
    pub skipped_users: i64,
}

const DEFAULT_LOOKBACK_DAYS: i64 = 6;

pub async fn run(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    only_user: Option<Uuid>,
) -> Result<RunReport, sqlx::Error> {
    let today = Utc::now().date_naive();
    let to = to.unwrap_or(today);
    let from = from.unwrap_or_else(|| to - Duration::days(DEFAULT_LOOKBACK_DAYS));

    let user_ids: Vec<Uuid> = match only_user {
        Some(id) => vec![id],
        None => {
            sqlx::query_scalar!("SELECT id FROM users ORDER BY id")
                .fetch_all(db)
                .await?
        }
    };

    let mut report = RunReport {
        days: (to - from).num_days() + 1,
        ..Default::default()
    };

    for user_id in user_ids {
        match run_for_user(db, fx, user_id, from, to).await {
            Ok((rows, unpriced)) => {
                report.users += 1;
                report.rows_upserted += rows;
                report.unpriced_rows += unpriced;
            }
            Err(error) => {
                tracing::warn!(%user_id, error = %error, "スナップショット生成をスキップ");
                report.skipped_users += 1;
            }
        }
    }

    Ok(report)
}

async fn run_for_user(
    db: &PgPool,
    fx: &dyn FxRateProvider,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> anyhow::Result<(i64, i64)> {
    let ctx = analytics_service::prepare(
        db,
        fx,
        user_id,
        from,
        to,
        analytics_service::Granularity::Day,
    )
    .await?;

    let mut tx = db.begin().await?;
    let mut rows_total = 0i64;
    let mut unpriced_total = 0i64;

    let mut day = from;
    while day <= to {
        let values = analytics_service::evaluate_context_day(&ctx, day);
        let rows: Vec<SnapshotRow> = values
            .iter()
            .map(|value| SnapshotRow {
                account_id: value.account_id,
                asset_id: value.asset_id,
                quantity: value.quantity,
                avg_cost: value.avg_cost,
                cost_basis_jpy: value.cost_basis_jpy,
                market_value_jpy: value.market_value_jpy,
                price: value.price,
                unpriced: value.market_value_jpy.is_none(),
            })
            .collect();

        unpriced_total += rows.iter().filter(|row| row.unpriced).count() as i64;
        rows_total += rows.len() as i64;
        snapshot_repo::upsert_day(&mut tx, user_id, day, &rows).await?;
        day += Duration::days(1);
    }

    tx.commit().await?;
    Ok((rows_total, unpriced_total))
}
