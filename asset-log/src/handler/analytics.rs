use axum::{
    Json,
    extract::{Query, State},
};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    middleware::auth::AuthUser,
    service::{
        allocation_service::{self, AllocationResult},
        analytics_service::{self, Granularity, GroupBy, HistoryResult},
    },
    state::AppState,
};

/// 1リクエストで返す点の上限。日次で約5年半。
const MAX_POINTS: i64 = 2000;

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    #[serde(default = "default_granularity")]
    pub granularity: Granularity,
    #[serde(default = "default_group_by")]
    pub group_by: GroupBy,
}

fn default_granularity() -> Granularity {
    Granularity::Day
}
fn default_group_by() -> GroupBy {
    GroupBy::None
}

pub async fn asset_history(
    State(st): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<HistoryResult>> {
    // JST の「今日」。タスク#8の取引登録と同じ基準に合わせる
    let jst = chrono::FixedOffset::east_opt(9 * 3600).expect("valid offset");
    let today = chrono::Utc::now().with_timezone(&jst).date_naive();

    let to = q.to.unwrap_or(today);
    let from = q.from.unwrap_or_else(|| to - chrono::Duration::days(365));

    if to > today {
        return Err(AppError::field("to", "未来の日付は指定できません"));
    }
    if from > to {
        return Err(AppError::field("from", "to より後の日付は指定できません"));
    }
    if (to - from).num_days() + 1 > MAX_POINTS {
        return Err(AppError::field(
            "from",
            "期間が長すぎます。granularity=month を使うか範囲を狭めてください",
        ));
    }

    let result = analytics_service::asset_history(
        &st.db,
        st.fx.as_ref(),
        user_id,
        from,
        to,
        q.granularity,
        q.group_by,
    )
    .await?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct AllocationQuery {
    pub as_of: Option<NaiveDate>,
    #[serde(default = "default_allocation_group_by")]
    pub group_by: GroupBy,
}

fn default_allocation_group_by() -> GroupBy {
    GroupBy::AssetClass
}

pub async fn allocation(
    State(st): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(q): Query<AllocationQuery>,
) -> AppResult<Json<AllocationResult>> {
    let jst = chrono::FixedOffset::east_opt(9 * 3600).expect("valid offset");
    let today = chrono::Utc::now().with_timezone(&jst).date_naive();

    let as_of = q.as_of.unwrap_or(today);
    if as_of > today {
        return Err(AppError::field("as_of", "未来の日付は指定できません"));
    }
    if q.group_by == GroupBy::None {
        return Err(AppError::field(
            "group_by",
            "allocation では none を指定できません",
        ));
    }

    let result =
        allocation_service::allocation(&st.db, st.fx.as_ref(), user_id, as_of, q.group_by).await?;

    Ok(Json(result))
}
