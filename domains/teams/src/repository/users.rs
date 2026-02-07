//! User repository

use crate::domain::entities::{User, UserTier};
use framecast_common::{RepositoryError, Result};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find user by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, name, avatar_url,
                   tier as "tier: UserTier", credits,
                   ephemeral_storage_bytes, upgraded_at,
                   created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Find user by email
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, name, avatar_url,
                   tier as "tier: UserTier", credits,
                   ephemeral_storage_bytes, upgraded_at,
                   created_at, updated_at
            FROM users
            WHERE email = $1
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Create new user
    pub async fn create(&self, user: &User) -> Result<User> {
        let created = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (
                id, email, name, avatar_url, tier, credits,
                ephemeral_storage_bytes, upgraded_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user.id,
            user.email,
            user.name,
            user.avatar_url,
            user.tier.clone() as UserTier,
            user.credits,
            user.ephemeral_storage_bytes,
            user.upgraded_at,
            user.created_at,
            user.updated_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                RepositoryError::AlreadyExists
            }
            _ => RepositoryError::from(e),
        })?;

        Ok(created)
    }

    /// Update user credits atomically
    pub async fn update_credits(&self, user_id: Uuid, credits_delta: i32) -> Result<User> {
        let updated = sqlx::query_as!(
            User,
            r#"
            UPDATE users SET
                credits = credits + $2,
                updated_at = NOW()
            WHERE id = $1 AND credits + $2 >= 0
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user_id,
            credits_delta
        )
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or(
            RepositoryError::ConstraintViolation("Credits would become negative".to_string())
                .into(),
        )
    }

    /// Update user profile (name, avatar_url)
    pub async fn update_profile(
        &self,
        user_id: Uuid,
        name: Option<String>,
        avatar_url: Option<String>,
    ) -> Result<Option<User>> {
        let updated = sqlx::query_as!(
            User,
            r#"
            UPDATE users SET
                name = $2,
                avatar_url = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user_id,
            name,
            avatar_url
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(updated)
    }

    /// Upgrade user tier
    pub async fn upgrade_tier(&self, user_id: Uuid, new_tier: UserTier) -> Result<Option<User>> {
        let now = chrono::Utc::now();
        let updated = sqlx::query_as!(
            User,
            r#"
            UPDATE users SET
                tier = $2,
                upgraded_at = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user_id,
            new_tier as UserTier,
            now
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(updated)
    }
}
