//! Artifact repository

use crate::domain::entities::{Artifact, ArtifactStatus};
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

/// All columns in the artifacts table, used for SELECT and RETURNING clauses.
const ARTIFACT_COLUMNS: &str = "\
    id, owner, created_by, project_id, \
    kind, status, source, \
    filename, s3_key, content_type, size_bytes, \
    spec, conversation_id, source_job_id, \
    metadata, created_at, updated_at";

#[derive(Clone)]
pub struct ArtifactRepository {
    pool: PgPool,
}

impl ArtifactRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find artifact by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<Artifact>> {
        let query = format!("SELECT {ARTIFACT_COLUMNS} FROM artifacts WHERE id = $1");
        let artifact = sqlx::query_as::<_, Artifact>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(artifact)
    }

    /// List artifacts by multiple owner URNs (user + teams)
    pub async fn list_by_owners(
        &self,
        owners: &[String],
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Artifact>> {
        let query = format!(
            "SELECT {ARTIFACT_COLUMNS} FROM artifacts \
             WHERE owner = ANY($1) ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        );
        let artifacts = sqlx::query_as::<_, Artifact>(&query)
            .bind(owners)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok(artifacts)
    }

    /// List artifacts by project ID
    pub async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Artifact>> {
        let query = format!(
            "SELECT {ARTIFACT_COLUMNS} FROM artifacts \
             WHERE project_id = $1 ORDER BY created_at DESC"
        );
        let artifacts = sqlx::query_as::<_, Artifact>(&query)
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(artifacts)
    }

    /// Create a new artifact
    pub async fn create(&self, artifact: &Artifact) -> Result<Artifact> {
        let query = format!(
            "INSERT INTO artifacts ({ARTIFACT_COLUMNS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
             RETURNING {ARTIFACT_COLUMNS}"
        );
        let created = sqlx::query_as::<_, Artifact>(&query)
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
            .bind(artifact.source_job_id)
            .bind(&artifact.metadata)
            .bind(artifact.created_at)
            .bind(artifact.updated_at)
            .fetch_one(&self.pool)
            .await?;

        Ok(created)
    }

    /// Update artifact status
    pub async fn update_status(
        &self,
        id: Uuid,
        status: ArtifactStatus,
    ) -> Result<Option<Artifact>> {
        let query = format!(
            "UPDATE artifacts SET status = $2, updated_at = NOW() \
             WHERE id = $1 RETURNING {ARTIFACT_COLUMNS}"
        );
        let updated = sqlx::query_as::<_, Artifact>(&query)
            .bind(id)
            .bind(status)
            .fetch_optional(&self.pool)
            .await?;

        Ok(updated)
    }

    /// Delete an artifact
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM artifacts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
