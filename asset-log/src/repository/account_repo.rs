use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::{Account, AccountPatch, AccountType, NewAccount};

pub async fn insert(
    db: &PgPool,
    user_id: Uuid,
    new: &NewAccount<'_>,
) -> Result<Account, sqlx::Error> {
    sqlx::query_as!(
        Account,
        r#"
        INSERT INTO accounts (user_id, name, account_type, withholding, institution, currency)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, user_id, name,
                  account_type AS "account_type: AccountType",
                  withholding, institution, currency, created_at, updated_at
        "#,
        user_id,
        new.name,
        new.account_type as AccountType,
        new.withholding,
        new.institution,
        new.currency,
    )
    .fetch_one(db)
    .await
}

pub async fn list(db: &PgPool, user_id: Uuid) -> Result<Vec<Account>, sqlx::Error> {
    sqlx::query_as!(
        Account,
        r#"
        SELECT id, user_id, name,
               account_type AS "account_type: AccountType",
               withholding, institution, currency, created_at, updated_at
        FROM accounts
        WHERE user_id = $1
        ORDER BY created_at
        "#,
        user_id,
    )
    .fetch_all(db)
    .await
}

pub async fn find(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as!(
        Account,
        r#"
        SELECT id, user_id, name,
               account_type AS "account_type: AccountType",
               withholding, institution, currency, created_at, updated_at
        FROM accounts
        WHERE id = $1 AND user_id = $2
        "#,
        id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

pub async fn update(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    patch: &AccountPatch<'_>,
) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as!(
        Account,
        r#"
        UPDATE accounts
        SET name        = COALESCE($3, name),
            institution = CASE WHEN $4 THEN $5 ELSE institution END,
            withholding = CASE WHEN $6 THEN $7 ELSE withholding END
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, name,
                  account_type AS "account_type: AccountType",
                  withholding, institution, currency, created_at, updated_at
        "#,
        id,
        user_id,
        patch.name,
        patch.institution.is_some(),
        patch.institution.flatten(),
        patch.withholding.is_some(),
        patch.withholding.flatten(),
    )
    .fetch_optional(db)
    .await
}

pub async fn delete(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM accounts WHERE id = $1 AND user_id = $2",
        id,
        user_id,
    )
    .execute(db)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// 口座名で1件引く。CSV取込で口座名からIDを解決するのに使う。
pub async fn find_by_name(
    db: &PgPool,
    user_id: Uuid,
    name: &str,
) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as!(
        Account,
        r#"
        SELECT id, user_id, name,
               account_type AS "account_type: AccountType",
               withholding, institution, currency, created_at, updated_at
        FROM accounts
        WHERE user_id = $1 AND name = $2
        "#,
        user_id,
        name,
    )
    .fetch_optional(db)
    .await
}
