use sqlx::PgPool;
use uuid::Uuid;

pub struct UserCredentials {
    pub id: Uuid,
    pub password_hash: String,
}

pub async fn insert(pool: &PgPool, email: &str, password_hash: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        r#"INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id"#,
        email,
        password_hash
    )
    .fetch_one(pool)
    .await
}

pub async fn find_credentials_by_email(
    pool: &PgPool,
    email: &str,
) -> Result<Option<UserCredentials>, sqlx::Error> {
    sqlx::query_as!(
        UserCredentials,
        r#"SELECT id, password_hash FROM users WHERE lower(email) = lower($1)"#,
        email
    )
    .fetch_optional(pool)
    .await
}
