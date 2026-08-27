use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::{
    domain::account::{Account, AccountPatch, AccountType, NewAccount},
    error::{AppError, AppResult},
    middleware::auth::AuthUser,
    repository::account_repo,
    state::AppState,
};
use utoipa::ToSchema;
// ---------- DTO ----------

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAccountRequest {
    pub name: String,
    pub account_type: AccountType,
    #[serde(default)]
    pub withholding: Option<bool>,
    #[serde(default)]
    pub institution: Option<String>,
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "JPY".to_owned()
}

/// 未指定 = None、`null` 明示 = Some(None)、値あり = Some(Some(v))
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountRequest {
    /// 口座名。未指定なら変更しない
    #[serde(default)]
    pub name: Option<String>,
    /// 金融機関名。未指定なら変更せず、`null` を明示すると削除する
    #[serde(default, deserialize_with = "double_option")]
    pub institution: Option<Option<String>>,
    /// 源泉徴収区分。未指定なら変更せず、`null` を明示すると削除する
    #[serde(default, deserialize_with = "double_option")]
    pub withholding: Option<Option<bool>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountResponse {
    pub id: Uuid,
    pub name: String,
    pub account_type: AccountType,
    pub withholding: Option<bool>,
    pub institution: Option<String>,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Account> for AccountResponse {
    fn from(a: Account) -> Self {
        // user_id はここで落とす
        Self {
            id: a.id,
            name: a.name,
            account_type: a.account_type,
            withholding: a.withholding,
            institution: a.institution,
            currency: a.currency,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

// ---------- ハンドラ ----------
#[utoipa::path(
    post,
    path = "/accounts",
    tag = "accounts",
    security(("bearerAuth" = [])),
    request_body = CreateAccountRequest,
    responses(
        (status = 201, description = "口座を作成した", body = AccountResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 409, description = "同じ名前の口座が既に存在する", body = ProblemDetails),
        (status = 422, description = "通貨コードの形式不正、口座名が空、源泉徴収区分の指定誤りなど", body = ProblemDetails)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<CreateAccountRequest>,
) -> AppResult<(StatusCode, Json<AccountResponse>)> {
    let new = NewAccount {
        name: body.name.trim(),
        account_type: body.account_type,
        withholding: body.withholding,
        institution: body.institution.as_deref(),
        currency: &body.currency,
    };

    let account = account_repo::insert(&state.db, user_id, &new).await?;
    Ok((StatusCode::CREATED, Json(account.into())))
}
#[utoipa::path(
    get,
    path = "/accounts",
    tag = "accounts",
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "口座の一覧", body = Vec<AccountResponse>),
        (status = 401, description = "認証が必要", body = ProblemDetails)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> AppResult<Json<Vec<AccountResponse>>> {
    let accounts = account_repo::list(&state.db, user_id).await?;
    Ok(Json(accounts.into_iter().map(Into::into).collect()))
}
#[utoipa::path(
    get,
    path = "/accounts/{id}",
    tag = "accounts",
    security(("bearerAuth" = [])),
    params(("id" = Uuid, Path, description = "口座ID")),
    responses(
        (status = 200, description = "口座の詳細", body = AccountResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 404, description = "口座が存在しない、または他ユーザーの口座", body = ProblemDetails)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AccountResponse>> {
    account_repo::find(&state.db, user_id, id)
        .await?
        .map(|a| Json(a.into()))
        .ok_or(AppError::NotFound("口座"))
}
#[utoipa::path(
    patch,
    path = "/accounts/{id}",
    tag = "accounts",
    security(("bearerAuth" = [])),
    params(("id" = Uuid, Path, description = "口座ID")),
    request_body = UpdateAccountRequest,
    responses(
        (status = 200, description = "更新後の口座", body = AccountResponse),
        (status = 400, description = "更新する項目が1つも指定されていない", body = ProblemDetails),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 404, description = "口座が存在しない", body = ProblemDetails),
        (status = 409, description = "同じ名前の口座が既に存在する", body = ProblemDetails),
        (status = 422, description = "入力値が制約を満たしていない", body = ProblemDetails)
    )
)]
pub async fn update(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAccountRequest>,
) -> AppResult<Json<AccountResponse>> {
    let name = body.name.as_deref().map(str::trim);
    let patch = AccountPatch {
        name,
        institution: body.institution.as_ref().map(|o| o.as_deref()),
        withholding: body.withholding,
    };

    if patch.is_empty() {
        return Err(AppError::BadRequest(
            "更新する項目を1つ以上指定してください".to_owned(),
        ));
    }

    account_repo::update(&state.db, user_id, id, &patch)
        .await?
        .map(|a| Json(a.into()))
        .ok_or(AppError::NotFound("口座"))
}
#[utoipa::path(
    delete,
    path = "/accounts/{id}",
    tag = "accounts",
    security(("bearerAuth" = [])),
    params(("id" = Uuid, Path, description = "口座ID")),
    responses(
        (status = 204, description = "削除した"),
        (status = 401, description = "認証が必要", body = ProblemDetails),
        (status = 404, description = "口座が存在しない", body = ProblemDetails),
        (status = 409, description = "取引が紐づいているため削除できない", body = ProblemDetails)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    if account_repo::delete(&state.db, user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("口座"))
    }
}
