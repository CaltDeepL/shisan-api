use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::{jwt::JwtKeys, password};
use crate::error::{AppError, AppResult, FieldError, OnConstraint};
use crate::middleware::auth::AuthUser;
use crate::repository::user_repository;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
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

/// 保護API の動作確認用。タスク#5以降のCRUDも同じ形で書ける。
pub async fn me(AuthUser(user_id): AuthUser) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({ "user_id": user_id })))
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
