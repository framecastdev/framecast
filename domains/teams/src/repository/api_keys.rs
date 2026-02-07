//! API Key repository

use crate::domain::entities::ApiKey;
use framecast_common::{RepositoryError, Result};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find API key by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<ApiKey>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, owner, name, key_prefix, key_hash,
                   scopes, last_used_at, expires_at, revoked_at, created_at
            FROM api_keys
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let scopes: Vec<String> = serde_json::from_value(row.scopes)
                .map_err(|e| RepositoryError::InvalidData(format!("Invalid scopes JSON: {}", e)))?;

            let api_key = ApiKey {
                id: row.id,
                user_id: row.user_id,
                owner: row.owner,
                name: row.name,
                key_prefix: row.key_prefix,
                key_hash: row.key_hash,
                scopes: sqlx::types::Json(scopes),
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
                created_at: row.created_at,
            };
            Ok(Some(api_key))
        } else {
            Ok(None)
        }
    }

    /// Authenticate by API key
    pub async fn authenticate(&self, candidate_key: &str) -> Result<Option<ApiKey>> {
        // Extract key prefix
        if !candidate_key.starts_with("sk_live_") {
            return Ok(None);
        }

        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, owner, name, key_prefix, key_hash,
                   scopes, last_used_at, expires_at, revoked_at, created_at
            FROM api_keys
            WHERE key_prefix = $1 AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            "sk_live_"
        )
        .fetch_all(&self.pool)
        .await?;

        // Find the matching key using constant-time verification
        for row in rows {
            let scopes: Vec<String> = serde_json::from_value(row.scopes)
                .map_err(|e| RepositoryError::InvalidData(format!("Invalid scopes JSON: {}", e)))?;

            let api_key = ApiKey {
                id: row.id,
                user_id: row.user_id,
                owner: row.owner,
                name: row.name,
                key_prefix: row.key_prefix,
                key_hash: row.key_hash,
                scopes: sqlx::types::Json(scopes),
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
                created_at: row.created_at,
            };

            if api_key.verify_key(candidate_key) {
                // Update last_used_at
                sqlx::query!(
                    "UPDATE api_keys SET last_used_at = NOW() WHERE id = $1",
                    api_key.id
                )
                .execute(&self.pool)
                .await?;

                return Ok(Some(api_key));
            }
        }

        Ok(None)
    }

    /// Create new API key
    pub async fn create(&self, api_key: &ApiKey) -> Result<ApiKey> {
        sqlx::query!(
            r#"
            INSERT INTO api_keys (
                id, user_id, owner, name, key_prefix, key_hash,
                scopes, last_used_at, expires_at, revoked_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            api_key.id,
            api_key.user_id,
            api_key.owner,
            api_key.name,
            api_key.key_prefix,
            api_key.key_hash,
            serde_json::to_value(&api_key.scopes.0)?,
            api_key.last_used_at,
            api_key.expires_at,
            api_key.revoked_at,
            api_key.created_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                RepositoryError::AlreadyExists
            }
            _ => RepositoryError::from(e),
        })?;

        Ok(api_key.clone())
    }

    /// Revoke API key
    pub async fn revoke(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query!(
            "UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
            id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound.into());
        }

        Ok(())
    }
}
