//! Transaction helpers for cross-domain artifact operations

use crate::domain::entities::Artifact;
use framecast_common::Result;
use sqlx::{Postgres, Transaction};

/// Create an artifact within an existing transaction.
/// Used by the conversations domain when creating artifacts from LLM responses.
pub async fn create_artifact_tx(
    tx: &mut Transaction<'_, Postgres>,
    artifact: &Artifact,
) -> Result<Artifact> {
    let created = sqlx::query_as::<_, Artifact>(
        r#"
        INSERT INTO artifacts (
            id, owner, created_by, project_id,
            kind, status, source,
            filename, s3_key, content_type, size_bytes,
            spec, conversation_id, source_generation_id,
            metadata, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        RETURNING id, owner, created_by, project_id,
                  kind, status, source,
                  filename, s3_key, content_type, size_bytes,
                  spec, conversation_id, source_generation_id,
                  metadata, created_at, updated_at
        "#,
    )
    .bind(artifact.id)
    .bind(&artifact.owner)
    .bind(artifact.created_by)
    .bind(artifact.project_id)
    .bind(artifact.kind)
    .bind(artifact.status)
    .bind(artifact.source)
    .bind(&artifact.filename)
    .bind(&artifact.s3_key)
    .bind(&artifact.content_type)
    .bind(artifact.size_bytes)
    .bind(&artifact.spec)
    .bind(artifact.conversation_id)
    .bind(artifact.source_generation_id)
    .bind(&artifact.metadata)
    .bind(artifact.created_at)
    .bind(artifact.updated_at)
    .fetch_one(&mut **tx)
    .await?;

    Ok(created)
}
