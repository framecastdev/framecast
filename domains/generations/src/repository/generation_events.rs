//! Generation event repository

use crate::domain::entities::{GenerationEventRecord, GenerationEventType};
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct GenerationEventRepository {
    pool: PgPool,
}

impl GenerationEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a generation event
    pub async fn create(
        &self,
        generation_id: Uuid,
        sequence: i64,
        event_type: GenerationEventType,
        payload: serde_json::Value,
    ) -> Result<GenerationEventRecord> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::create_generation_event_tx(
            &mut tx,
            generation_id,
            sequence,
            event_type,
            payload,
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// List events for a generation, optionally after a specific sequence number
    pub async fn list_by_generation(
        &self,
        generation_id: Uuid,
        after_sequence: Option<i64>,
    ) -> Result<Vec<GenerationEventRecord>> {
        let after = after_sequence.unwrap_or(0);
        let rows = sqlx::query_as::<_, GenerationEventRecord>(
            r#"
            SELECT id, generation_id, sequence, event_type, payload, created_at
            FROM generation_events
            WHERE generation_id = $1 AND sequence > $2
            ORDER BY sequence ASC
            "#,
        )
        .bind(generation_id)
        .bind(after)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get the next sequence number for a generation
    pub async fn next_sequence(&self, generation_id: Uuid) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let row = super::transactions::next_sequence_tx(&mut tx, generation_id).await?;
        tx.commit().await?;
        Ok(row)
    }
}
