//! Job event repository

use crate::domain::entities::{JobEventRecord, JobEventType};
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct JobEventRepository {
    pool: PgPool,
}

impl JobEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a job event
    pub async fn create(
        &self,
        job_id: Uuid,
        sequence: i64,
        event_type: JobEventType,
        payload: serde_json::Value,
    ) -> Result<JobEventRecord> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::create_job_event_tx(
            &mut tx, job_id, sequence, event_type, payload,
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// List events for a job, optionally after a specific sequence number
    pub async fn list_by_job(
        &self,
        job_id: Uuid,
        after_sequence: Option<i64>,
    ) -> Result<Vec<JobEventRecord>> {
        let after = after_sequence.unwrap_or(0);
        let rows = sqlx::query_as::<_, JobEventRecord>(
            r#"
            SELECT id, job_id, sequence, event_type, payload, created_at
            FROM job_events
            WHERE job_id = $1 AND sequence > $2
            ORDER BY sequence ASC
            "#,
        )
        .bind(job_id)
        .bind(after)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get the next sequence number for a job
    pub async fn next_sequence(&self, job_id: Uuid) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::next_sequence_tx(&mut tx, job_id).await?;
        tx.commit().await?;
        Ok(row)
    }
}
