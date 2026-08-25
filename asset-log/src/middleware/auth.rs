use axum::RequestPartsExt;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum_extra::TypedHeader;
use axum_extra::headers::{Authorization, authorization::Bearer};
use uuid::Uuid;

use crate::auth::job::JobToken;
use crate::auth::jwt::JwtKeys;
use crate::error::AppError;

/// これを引数に取ったハンドラは自動的に保護される。
#[derive(Debug, Clone, Copy)]
pub struct AuthUser(pub Uuid);

impl<S> FromRequestParts<S> for AuthUser
where
    JwtKeys: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let keys = JwtKeys::from_ref(state);
        let claims = keys.verify(bearer.token()).ok_or(AppError::Unauthorized)?;

        Ok(AuthUser(claims.sub))
    }
}
/// バッチ実行専用の認証マーカー。ユーザーを特定しない（全ユーザー対象の処理で使う）
#[derive(Debug, Clone, Copy)]
pub struct JobAuth;

impl<S> FromRequestParts<S> for JobAuth
where
    JobToken: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        JobToken::from_ref(state).verify(bearer.token())?;

        Ok(JobAuth)
    }
}
