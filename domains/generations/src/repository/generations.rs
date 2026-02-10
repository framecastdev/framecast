//! Generation repository

use crate::domain::entities::{Generation, GenerationStatus};
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

/// All columns in the generations table, used for SELECT and RETURNING clauses.
pub(crate) const GENERATION_COLUMNS: &str = "id, owner, triggered_by, project_id, status, spec_snapshot, options, progress, output, output_size_bytes, error, credits_charged, failure_type, credits_refunded, idempotency_key, started_at, completed_at, created_at, updated_at";

#[derive(Clone)]
pub struct GenerationRepository {
    pool: PgPool,
}

impl GenerationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find generation by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<Generation>> {
        let query = format!("SELECT {GENERATION_COLUMNS} FROM generations WHERE id = $1");
        let row = sqlx::query_as::<_, Generation>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    /// List generations by owner URNs with optional filters
    pub async fn list_by_owners(
        &self,
        owners: &[String],
        status_filter: Option<&GenerationStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Generation>> {
        // Build dynamic query
        let mut query =
            format!("SELECT {GENERATION_COLUMNS} FROM generations WHERE owner = ANY($1)");

        if status_filter.is_some() {
            query.push_str(" AND status = $4");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT $2 OFFSET $3");

        if let Some(status) = status_filter {
            let rows = sqlx::query_as::<_, Generation>(&query)
                .bind(owners)
                .bind(limit)
                .bind(offset)
                .bind(status)
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        } else {
            let rows = sqlx::query_as::<_, Generation>(&query)
                .bind(owners)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        }
    }

    /// Create a new generation
    pub async fn create(&self, generation: &Generation) -> Result<Generation> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::create_generation_tx(&mut tx, generation).await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Update an existing generation
    pub async fn update(&self, generation: &Generation) -> Result<Generation> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::update_generation_tx(&mut tx, generation).await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Delete a generation by ID
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        // Clear artifact references (source_generation_consistency CHECK requires
        // source_generation_id IS NOT NULL when source = 'generation', so clear both)
        sqlx::query(
            "UPDATE artifacts SET source_generation_id = NULL, source = 'upload'::artifact_source WHERE source_generation_id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        // Delete generation events first (FK constraint)
        sqlx::query("DELETE FROM generation_events WHERE generation_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let result = sqlx::query("DELETE FROM generations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count active (non-terminal) generations for an owner
    pub async fn count_active_for_owner(&self, owner: &str) -> Result<i64> {
        let row = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM generations WHERE owner = $1 AND status IN ('queued', 'processing')",
        )
        .bind(owner)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Find generation by idempotency key for a specific user
    pub async fn find_by_idempotency_key(
        &self,
        triggered_by: Uuid,
        key: &str,
    ) -> Result<Option<Generation>> {
        let query = format!(
            "SELECT {GENERATION_COLUMNS} FROM generations WHERE triggered_by = $1 AND idempotency_key = $2"
        );
        let row = sqlx::query_as::<_, Generation>(&query)
            .bind(triggered_by)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }
}
