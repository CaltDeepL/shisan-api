use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use uuid::Uuid;

/// Postgres SQLSTATE（https://www.postgresql.org/docs/current/errcodes-appendix.html）
mod sqlstate {
    pub const NOT_NULL_VIOLATION: &str = "23502";
    pub const FOREIGN_KEY_VIOLATION: &str = "23503";
    pub const UNIQUE_VIOLATION: &str = "23505";
    pub const CHECK_VIOLATION: &str = "23514";
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("認証が必要です")]
    Unauthorized,

    #[error("この操作は許可されていません")]
    Forbidden,

    #[error("{0}が見つかりません")]
    NotFound(&'static str),

    #[error("{0}")]
    Conflict(String),

    /// 422: 形式は正しいが内容が受け付けられない（CHECK 制約違反・バリデーション）
    #[error("{detail}")]
    UnprocessableEntity {
        detail: String,
        errors: Vec<FieldError>,
    },

    /// 500: 分類できなかった DB エラー
    #[error("database error")]
    Database(#[source] sqlx::Error),

    /// 500: それ以外
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl AppError {
    pub fn unprocessable(detail: impl Into<String>) -> Self {
        Self::UnprocessableEntity {
            detail: detail.into(),
            errors: Vec::new(),
        }
    }

    pub fn field(field: impl Into<String>, message: impl Into<String>) -> Self {
        let message = message.into();
        Self::UnprocessableEntity {
            detail: "入力値が不正です".to_owned(),
            errors: vec![FieldError {
                field: field.into(),
                message,
            }],
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::UnprocessableEntity { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// RFC 9457 の `type`。安定した識別子としてクライアントが分岐に使える
    fn error_type(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad-request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound(_) => "not-found",
            Self::Conflict(_) => "conflict",
            Self::UnprocessableEntity { .. } => "unprocessable-entity",
            Self::Database(_) | Self::Internal(_) => "internal-error",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "Bad Request",
            Self::Unauthorized => "Unauthorized",
            Self::Forbidden => "Forbidden",
            Self::NotFound(_) => "Not Found",
            Self::Conflict(_) => "Conflict",
            Self::UnprocessableEntity { .. } => "Unprocessable Entity",
            Self::Database(_) | Self::Internal(_) => "Internal Server Error",
        }
    }
}

// ---------- sqlx::Error の分類 ----------

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::RowNotFound) {
            return Self::NotFound("リソース");
        }

        let Some(db_err) = err.as_database_error() else {
            return Self::Database(err);
        };
        let constraint = db_err.constraint();

        match db_err.code().as_deref() {
            Some(sqlstate::UNIQUE_VIOLATION) => {
                Self::Conflict(unique_message(constraint).to_owned())
            }
            Some(sqlstate::CHECK_VIOLATION) => Self::unprocessable(check_message(constraint)),
            Some(sqlstate::FOREIGN_KEY_VIOLATION) => {
                Self::unprocessable("指定された関連リソースが存在しません")
            }
            Some(sqlstate::NOT_NULL_VIOLATION) => Self::unprocessable("必須項目が未入力です"),
            _ => Self::Database(err),
        }
    }
}

/// 制約名 → ユーザー向けメッセージ。migration を追加したらここも増やす
fn unique_message(constraint: Option<&str>) -> &'static str {
    match constraint {
        Some("users_email_lower_key") => "このメールアドレスは既に登録されています",
        Some("accounts_user_name_key") => "同じ名前の口座が既に存在します",
        _ => "既に登録されている値です",
    }
}


fn check_message(constraint: Option<&str>) -> &'static str {
    match constraint {
        Some("accounts_currency_format") => "通貨コードは ISO 4217 の大文字3文字で指定してください",
        Some("accounts_name_not_blank") => "口座名を空にはできません",
        Some("accounts_withholding_only_tokutei") => {
            "源泉徴収区分は特定口座のみ指定できます（特定口座では必須です）"
        }
        _ => "入力値が制約を満たしていません",
    }
}

/// 個別に文言を差し替えたいときに使う
pub trait OnConstraint<T> {
    fn on_constraint(
        self,
        name: &str,
        f: impl FnOnce() -> AppError,
    ) -> Result<T, AppError>;
}

impl<T> OnConstraint<T> for Result<T, sqlx::Error> {
    fn on_constraint(
        self,
        name: &str,
        f: impl FnOnce() -> AppError,
    ) -> Result<T, AppError> {
        self.map_err(|err| {
            if err.as_database_error().and_then(|e| e.constraint()) == Some(name) {
                f()
            } else {
                err.into()
            }
        })
    }
}

// ---------- レスポンス化 ----------

#[derive(Serialize)]
struct ProblemDetails<'a> {
    /// RFC 9457 では URI 参照。ここでは相対参照を使う
    #[serde(rename = "type")]
    kind: String,
    title: &'a str,
    status: u16,
    detail: String,
    #[serde(skip_serializing_if = "<[FieldError]>::is_empty")]
    errors: &'a [FieldError],
    trace_id: Uuid,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let trace_id = Uuid::new_v4();

        if status.is_server_error() {
            // source まで出すため Debug を使う
            tracing::error!(%trace_id, error = ?self, "internal error");
        } else {
            tracing::warn!(%trace_id, status = status.as_u16(), error = %self, "request rejected");
        }

        let empty: Vec<FieldError> = Vec::new();
        let (detail, errors) = match &self {
            // 5xx は内部情報を返さない
            Self::Database(_) | Self::Internal(_) => (
                "サーバー内部でエラーが発生しました".to_owned(),
                &empty,
            ),
            Self::UnprocessableEntity { detail, errors } => (detail.clone(), errors),
            other => (other.to_string(), &empty),
        };

        let body = ProblemDetails {
            kind: format!("/errors/{}", self.error_type()),
            title: self.title(),
            status: status.as_u16(),
            detail,
            errors,
            trace_id,
        };

        (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(body),
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;