//! Transaction helpers for Generations domain

use super::generations::GENERATION_COLUMNS;
use crate::domain::entities::{Generation, GenerationEventRecord, GenerationEventType};
use sqlx::{Postgres, Transaction};

/// Count active (non-terminal) generations for an owner within a transaction.
/// Uses a subquery with `FOR UPDATE` to lock matching rows and prevent
/// concurrent inserts from bypassing the concurrency limit (CARD-5 / CARD-6).
/// Note: `FOR UPDATE` cannot be combined directly with aggregate functions
/// in PostgreSQL, so we lock rows in the subquery and count in the outer query.
pub async fn count_active_for_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    owner: &str,
) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM (SELECT id FROM generations WHERE owner = $1 AND status IN ('queued', 'processing') FOR UPDATE) AS locked",
    )
    .bind(owner)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

/// Update an existing generation within a transaction
pub async fn update_generation_tx(
    tx: &mut Transaction<'_, Postgres>,
    generation: &Generation,
) -> Result<Generation, sqlx::Error> {
    let query = format!(
        "UPDATE generations SET \
            status = $2, progress = $3, output = $4, output_size_bytes = $5, \
            error = $6, failure_type = $7, credits_refunded = $8, \
            started_at = $9, completed_at = $10, updated_at = NOW() \
         WHERE id = $1 \
         RETURNING {GENERATION_COLUMNS}"
    );
    let row = sqlx::query_as::<_, Generation>(&query)
        .bind(generation.id)
        .bind(&generation.status)
        .bind(&generation.progress)
        .bind(&generation.output)
        .bind(generation.output_size_bytes)
        .bind(&generation.error)
        .bind(&generation.failure_type)
        .bind(generation.credits_refunded)
        .bind(generation.started_at)
        .bind(generation.completed_at)
        .fetch_one(&mut **tx)
        .await?;
    Ok(row)
}

/// Get the next sequence number for a generation within a transaction
pub async fn next_sequence_tx(
    tx: &mut Transaction<'_, Postgres>,
    generation_id: uuid::Uuid,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM generation_events WHERE generation_id = $1",
    )
    .bind(generation_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// Create a generation within a transaction
pub async fn create_generation_tx(
    tx: &mut Transaction<'_, Postgres>,
    generation: &Generation,
) -> Result<Generation, sqlx::Error> {
    let query = format!(
        "INSERT INTO generations ({GENERATION_COLUMNS}) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) \
         RETURNING {GENERATION_COLUMNS}"
    );
    let row = sqlx::query_as::<_, Generation>(&query)
        .bind(generation.id)
        .bind(&generation.owner)
        .bind(generation.triggered_by)
        .bind(generation.project_id)
        .bind(&generation.status)
        .bind(&generation.spec_snapshot)
        .bind(&generation.options)
        .bind(&generation.progress)
        .bind(&generation.output)
        .bind(generation.output_size_bytes)
        .bind(&generation.error)
        .bind(generation.credits_charged)
        .bind(&generation.failure_type)
        .bind(generation.credits_refunded)
        .bind(&generation.idempotency_key)
        .bind(generation.started_at)
        .bind(generation.completed_at)
        .bind(generation.created_at)
        .bind(generation.updated_at)
        .fetch_one(&mut **tx)
        .await?;
    Ok(row)
}

/// Create a generation event within a transaction
pub async fn create_generation_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    generation_id: uuid::Uuid,
    sequence: i64,
    event_type: GenerationEventType,
    payload: serde_json::Value,
) -> Result<GenerationEventRecord, sqlx::Error> {
    let row = sqlx::query_as::<_, GenerationEventRecord>(
        r#"
        INSERT INTO generation_events (generation_id, sequence, event_type, payload)
        VALUES ($1, $2, $3, $4)
        RETURNING id, generation_id, sequence, event_type, payload, created_at
        "#,
    )
    .bind(generation_id)
    .bind(sequence)
    .bind(&event_type)
    .bind(sqlx::types::Json(payload))
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// CQRS cross-domain write: Update artifact status by source generation ID
/// This writes to the artifacts table directly (same DB, different domain).
/// When `size_bytes` is provided (e.g. on completion), the artifact's size_bytes is updated too.
pub async fn update_artifact_status_by_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation_id: uuid::Uuid,
    status: &str,
    size_bytes: Option<i64>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE artifacts
        SET status = $2::asset_status,
            size_bytes = COALESCE($3, size_bytes),
            updated_at = NOW()
        WHERE source_generation_id = $1
        "#,
    )
    .bind(generation_id)
    .bind(status)
    .bind(size_bytes)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected())
}
