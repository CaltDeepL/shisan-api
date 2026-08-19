use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::{
    domain::account::{Account, AccountPatch, AccountType, NewAccount},
    error::{AppError, AppResult},
    middleware::auth::AuthUser,
    repository::account_repo,
    state::AppState,
};

// ---------- DTO ----------

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub institution: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub withholding: Option<Option<bool>>,
}

#[derive(Debug, Serialize)]
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

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> AppResult<Json<Vec<AccountResponse>>> {
    let accounts = account_repo::list(&state.db, user_id).await?;
    Ok(Json(accounts.into_iter().map(Into::into).collect()))
}

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