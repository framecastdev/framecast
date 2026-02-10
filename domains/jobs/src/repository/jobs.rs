//! Job repository

use crate::domain::entities::{Job, JobStatus};
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

/// All columns in the jobs table, used for SELECT and RETURNING clauses.
pub(crate) const JOB_COLUMNS: &str = "id, owner, triggered_by, project_id, status, spec_snapshot, options, progress, output, output_size_bytes, error, credits_charged, failure_type, credits_refunded, idempotency_key, started_at, completed_at, created_at, updated_at";

#[derive(Clone)]
pub struct JobRepository {
    pool: PgPool,
}

impl JobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find job by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<Job>> {
        let query = format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = $1");
        let row = sqlx::query_as::<_, Job>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    /// List jobs by owner URNs with optional filters
    pub async fn list_by_owners(
        &self,
        owners: &[String],
        status_filter: Option<&JobStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Job>> {
        // Build dynamic query
        let mut query = format!("SELECT {JOB_COLUMNS} FROM jobs WHERE owner = ANY($1)");

        if status_filter.is_some() {
            query.push_str(" AND status = $4");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT $2 OFFSET $3");

        if let Some(status) = status_filter {
            let rows = sqlx::query_as::<_, Job>(&query)
                .bind(owners)
                .bind(limit)
                .bind(offset)
                .bind(status)
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        } else {
            let rows = sqlx::query_as::<_, Job>(&query)
                .bind(owners)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        }
    }

    /// Create a new job
    pub async fn create(&self, job: &Job) -> Result<Job> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::create_job_tx(&mut tx, job).await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Update an existing job
    pub async fn update(&self, job: &Job) -> Result<Job> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::update_job_tx(&mut tx, job).await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Delete a job by ID
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        // Clear artifact references (source_job_consistency CHECK requires
        // source_job_id IS NOT NULL when source = 'job', so clear both)
        sqlx::query(
            "UPDATE artifacts SET source_job_id = NULL, source = 'upload'::artifact_source WHERE source_job_id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        // Delete job events first (FK constraint)
        sqlx::query("DELETE FROM job_events WHERE job_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let result = sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count active (non-terminal) jobs for an owner
    pub async fn count_active_for_owner(&self, owner: &str) -> Result<i64> {
        let row = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM jobs WHERE owner = $1 AND status IN ('queued', 'processing')",
        )
        .bind(owner)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Find job by idempotency key for a specific user
    pub async fn find_by_idempotency_key(
        &self,
        triggered_by: Uuid,
        key: &str,
    ) -> Result<Option<Job>> {
        let query = format!(
            "SELECT {JOB_COLUMNS} FROM jobs WHERE triggered_by = $1 AND idempotency_key = $2"
        );
        let row = sqlx::query_as::<_, Job>(&query)
            .bind(triggered_by)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }
}
