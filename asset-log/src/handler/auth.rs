use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::{jwt::JwtKeys, password};
use crate::error::{AppError, AppResult, FieldError, OnConstraint};
use crate::middleware::auth::AuthUser;
use crate::openapi::ProblemDetailsSchema as ProblemDetails;
use crate::repository::user_repository;
use crate::state::AppState;
use utoipa::ToSchema;
/// ログイン・登録の認証情報
#[derive(Deserialize, ToSchema)]
pub struct Credentials {
    /// メールアドレス（大文字小文字は区別しない）
    #[schema(example = "user@example.com")]
    pub email: String,
    /// パスワード（登録時は12文字以上）
    #[schema(example = "correct-horse-battery-staple")]
    pub password: String,
}
#[derive(Serialize, ToSchema)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}
/// 認証済みユーザーの情報
#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    pub user_id: uuid::Uuid,
}

fn field_error(field: &str, message: &str) -> FieldError {
    FieldError {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

fn validate(c: &Credentials) -> Result<String, AppError> {
    let email = c.email.trim().to_lowercase();
    let mut errors: Vec<FieldError> = Vec::new();

    if !email.contains('@') || email.len() > 254 {
        errors.push(field_error(
            "email",
            "メールアドレスの形式が正しくありません",
        ));
    }
    if c.password.chars().count() < 12 {
        errors.push(field_error(
            "password",
            "パスワードは12文字以上にしてください",
        ));
    }
    if c.password.len() > 1024 {
        errors.push(field_error("password", "パスワードが長すぎます"));
    }

    if errors.is_empty() {
        Ok(email)
    } else {
        Err(AppError::UnprocessableEntity {
            detail: "入力内容を確認してください".into(),
            errors,
        })
    }
}
#[utoipa::path(
    post,
    path = "/auth/register",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 201, description = "登録成功。アクセストークンを返す", body = TokenResponse),
        (status = 409, description = "このメールアドレスは既に登録されている", body = ProblemDetails),
        (status = 422, description = "入力値が要件を満たしていない", body = ProblemDetails)
    )
)]

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> AppResult<(StatusCode, Json<TokenResponse>)> {
    let email = validate(&body)?;

    let hash = tokio::task::spawn_blocking(move || password::hash_password(&body.password))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("join error: {e}")))??;

    let user_id = user_repository::insert(&state.db, &email, &hash)
        .await
        .on_constraint("users_email_lower_key", || {
            AppError::Conflict("このメールアドレスは既に登録されています".into())
        })?;

    Ok((StatusCode::CREATED, Json(issue(&state.jwt, user_id)?)))
}
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 200, description = "ログイン成功", body = TokenResponse),
        (status = 401, description = "メールアドレスまたはパスワードが違う", body = ProblemDetails)
    )
)]

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> AppResult<Json<TokenResponse>> {
    // login では validate を通さない。
    // 「12文字未満です」と返すと、登録済みパスワードの条件を漏らすため。
    let email = body.email.trim().to_lowercase();

    let found = user_repository::find_credentials_by_email(&state.db, &email).await?;

    let plain = body.password;
    let verified = tokio::task::spawn_blocking(move || match &found {
        Some(u) => password::verify_password(&plain, &u.password_hash).then_some(u.id),
        None => {
            password::verify_dummy(&plain); // 時間を揃えるためだけに実行
            None
        }
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("join error: {e}")))?;

    let user_id = verified.ok_or(AppError::InvalidCredentials)?;
    Ok(Json(issue(&state.jwt, user_id)?))
}
#[utoipa::path(
    get,
    path = "/me",
    tag = "auth",
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "認証済みユーザーの情報", body = MeResponse),
        (status = 401, description = "認証が必要", body = ProblemDetails)
    )
)]
/// 保護API の動作確認用。タスク#5以降のCRUDも同じ形で書ける。
pub async fn me(AuthUser(user_id): AuthUser) -> AppResult<Json<MeResponse>> {
    Ok(Json(MeResponse { user_id }))
}

fn issue(keys: &JwtKeys, user_id: uuid::Uuid) -> Result<TokenResponse, AppError> {
    let (access_token, expires_in) = keys
        .issue(user_id)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in,
    })
}
