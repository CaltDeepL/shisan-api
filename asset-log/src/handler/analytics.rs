use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::{
    error::{AppError, AppResult},
    middleware::auth::AuthUser,
    service::{
        allocation_service::{self, AllocationResult},
        analytics_service::{self, Granularity, GroupBy, HistoryResult},
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
};
use chrono::NaiveDate;
use serde::Deserialize;
use utoipa::IntoParams;

/// 1リクエストで返す点の上限。日次で約5年半。
const MAX_POINTS: i64 = 2000;

#[derive(Debug, Deserialize, IntoParams)]
pub struct HistoryQuery {
    /// 開始日。既定は to の365日前
    pub from: Option<NaiveDate>,
    /// 終了日。既定は当日（JST）。未来日は不可
    pub to: Option<NaiveDate>,
    /// 既定は day。期間が2000点を超える場合は month を使う
    #[serde(default = "default_granularity")]
    pub granularity: Granularity,
    /// 分類軸。既定は none（全体合計の1系列）
    #[serde(default = "default_group_by")]
    pub group_by: GroupBy,
}

fn default_granularity() -> Granularity {
    Granularity::Day
}
fn default_group_by() -> GroupBy {
    GroupBy::None
}
#[utoipa::path(
    get, path = "/analytics/asset-history", operation_id = "get_asset_history", tag = "analytics",
    security(("bearerAuth" = [])),
    params(HistoryQuery),
    responses(
        (status = 200, description = "資産評価額の時系列。日次スナップショットが揃っていれば source=snapshot、なければ再計算して source=computed", body = HistoryResult),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 422, description = "未来日、from が to より後、期間が2000点を超える", body = ProblemDetails),
        (status = 503, description = "為替レートを取得できず、キャッシュにも無い", body = ProblemDetails)
    )
)]
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

#[derive(Debug, Deserialize, IntoParams)]
pub struct AllocationQuery {
    /// 基準日。既定は当日（JST）。未来日は不可
    pub as_of: Option<NaiveDate>,
    /// 分類軸。既定は asset_class。none は指定できない
    #[serde(default = "default_allocation_group_by")]
    pub group_by: GroupBy,
}

fn default_allocation_group_by() -> GroupBy {
    GroupBy::AssetClass
}
#[utoipa::path(
    get, path = "/analytics/allocation", operation_id = "get_allocation", tag = "analytics",
    security(("bearerAuth" = [])),
    params(AllocationQuery),
    responses(
        (status = 200, description = "指定日時点のアセットアロケーション。ratio の合計は必ず100.00", body = AllocationResult),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 422, description = "未来日、group_by=none の指定", body = ProblemDetails),
        (status = 503, description = "為替レートを取得できず、キャッシュにも無い", body = ProblemDetails)
    )
)]
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
