//! System asset repository

use crate::domain::entities::SystemAsset;
use framecast_common::Result;
use sqlx::PgPool;

#[derive(Clone)]
pub struct SystemAssetRepository {
    pool: PgPool,
}

impl SystemAssetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find system asset by ID
    pub async fn find(&self, id: &str) -> Result<Option<SystemAsset>> {
        let asset = sqlx::query_as::<_, SystemAsset>(
            r#"
            SELECT id, category, name, description,
                   duration_seconds, s3_key, content_type,
                   size_bytes, tags, created_at
            FROM system_assets
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(asset)
    }

    /// List all system assets
    pub async fn list(&self) -> Result<Vec<SystemAsset>> {
        let assets = sqlx::query_as::<_, SystemAsset>(
            r#"
            SELECT id, category, name, description,
                   duration_seconds, s3_key, content_type,
                   size_bytes, tags, created_at
            FROM system_assets
            ORDER BY category, name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(assets)
    }

    /// List system assets by category
    pub async fn list_by_category(
        &self,
        category: &crate::domain::entities::SystemAssetCategory,
    ) -> Result<Vec<SystemAsset>> {
        let assets = sqlx::query_as::<_, SystemAsset>(
            r#"
            SELECT id, category, name, description,
                   duration_seconds, s3_key, content_type,
                   size_bytes, tags, created_at
            FROM system_assets
            WHERE category = $1
            ORDER BY name
            "#,
        )
        .bind(category)
        .fetch_all(&self.pool)
        .await?;

        Ok(assets)
    }
}
