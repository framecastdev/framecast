//! User repository

use crate::domain::entities::{User, UserTier};
use framecast_common::Result;
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

    /// Get user by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<User>> {
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
