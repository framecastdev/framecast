//! Transaction helpers for Jobs domain

use super::jobs::JOB_COLUMNS;
use crate::domain::entities::{Job, JobEventRecord, JobEventType};
use sqlx::{Postgres, Transaction};

/// Count active (non-terminal) jobs for an owner within a transaction.
/// Uses a subquery with `FOR UPDATE` to lock matching rows and prevent
/// concurrent inserts from bypassing the concurrency limit (CARD-5 / CARD-6).
/// Note: `FOR UPDATE` cannot be combined directly with aggregate functions
/// in PostgreSQL, so we lock rows in the subquery and count in the outer query.
pub async fn count_active_for_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    owner: &str,
) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM (SELECT id FROM jobs WHERE owner = $1 AND status IN ('queued', 'processing') FOR UPDATE) AS locked",
    )
    .bind(owner)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

/// Update an existing job within a transaction
pub async fn update_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    job: &Job,
) -> Result<Job, sqlx::Error> {
    let query = format!(
        "UPDATE jobs SET \
            status = $2, progress = $3, output = $4, output_size_bytes = $5, \
            error = $6, failure_type = $7, credits_refunded = $8, \
            started_at = $9, completed_at = $10, updated_at = NOW() \
         WHERE id = $1 \
         RETURNING {JOB_COLUMNS}"
    );
    let row = sqlx::query_as::<_, Job>(&query)
        .bind(job.id)
        .bind(&job.status)
        .bind(&job.progress)
        .bind(&job.output)
        .bind(job.output_size_bytes)
        .bind(&job.error)
        .bind(&job.failure_type)
        .bind(job.credits_refunded)
        .bind(job.started_at)
        .bind(job.completed_at)
        .fetch_one(&mut **tx)
        .await?;
    Ok(row)
}

/// Get the next sequence number for a job within a transaction
pub async fn next_sequence_tx(
    tx: &mut Transaction<'_, Postgres>,
    job_id: uuid::Uuid,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM job_events WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// Create a job within a transaction
pub async fn create_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    job: &Job,
) -> Result<Job, sqlx::Error> {
    let query = format!(
        "INSERT INTO jobs ({JOB_COLUMNS}) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) \
         RETURNING {JOB_COLUMNS}"
    );
    let row = sqlx::query_as::<_, Job>(&query)
        .bind(job.id)
        .bind(&job.owner)
        .bind(job.triggered_by)
        .bind(job.project_id)
        .bind(&job.status)
        .bind(&job.spec_snapshot)
        .bind(&job.options)
        .bind(&job.progress)
        .bind(&job.output)
        .bind(job.output_size_bytes)
        .bind(&job.error)
        .bind(job.credits_charged)
        .bind(&job.failure_type)
        .bind(job.credits_refunded)
        .bind(&job.idempotency_key)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(job.created_at)
        .bind(job.updated_at)
        .fetch_one(&mut **tx)
        .await?;
    Ok(row)
}

/// Create a job event within a transaction
pub async fn create_job_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    job_id: uuid::Uuid,
    sequence: i64,
    event_type: JobEventType,
    payload: serde_json::Value,
) -> Result<JobEventRecord, sqlx::Error> {
    let row = sqlx::query_as::<_, JobEventRecord>(
        r#"
        INSERT INTO job_events (job_id, sequence, event_type, payload)
        VALUES ($1, $2, $3, $4)
        RETURNING id, job_id, sequence, event_type, payload, created_at
        "#,
    )
    .bind(job_id)
    .bind(sequence)
    .bind(&event_type)
    .bind(sqlx::types::Json(payload))
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// CQRS cross-domain write: Update artifact status by source job ID
/// This writes to the artifacts table directly (same DB, different domain).
/// When `size_bytes` is provided (e.g. on completion), the artifact's size_bytes is updated too.
pub async fn update_artifact_status_by_job(
    tx: &mut Transaction<'_, Postgres>,
    job_id: uuid::Uuid,
    status: &str,
    size_bytes: Option<i64>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE artifacts
        SET status = $2::asset_status,
            size_bytes = COALESCE($3, size_bytes),
            updated_at = NOW()
        WHERE source_job_id = $1
        "#,
    )
    .bind(job_id)
    .bind(status)
    .bind(size_bytes)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected())
}
