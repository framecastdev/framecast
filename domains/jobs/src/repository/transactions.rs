//! Transaction helpers for Jobs domain

use crate::domain::entities::{Job, JobEventRecord, JobEventType};
use sqlx::{Postgres, Transaction};

/// Create a job within a transaction
pub async fn create_job_tx(
    tx: &mut Transaction<'_, Postgres>,
    job: &Job,
) -> Result<Job, sqlx::Error> {
    let row = sqlx::query_as::<_, Job>(
        r#"
        INSERT INTO jobs (id, owner, triggered_by, project_id, status, spec_snapshot, options,
                          progress, output, output_size_bytes, error, credits_charged,
                          failure_type, credits_refunded, idempotency_key,
                          started_at, completed_at, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
        RETURNING id, owner, triggered_by, project_id, status, spec_snapshot, options, progress,
                  output, output_size_bytes, error, credits_charged, failure_type,
                  credits_refunded, idempotency_key, started_at, completed_at, created_at, updated_at
        "#,
    )
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
/// This writes to the artifacts table directly (same DB, different domain)
pub async fn update_artifact_status_by_job(
    tx: &mut Transaction<'_, Postgres>,
    job_id: uuid::Uuid,
    status: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE artifacts SET status = $2::asset_status, updated_at = NOW() WHERE source_job_id = $1",
    )
    .bind(job_id)
    .bind(status)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected())
}
