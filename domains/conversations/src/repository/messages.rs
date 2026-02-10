//! Message repository

use crate::domain::entities::Message;
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct MessageRepository {
    pool: PgPool,
}

impl MessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List messages for a conversation, ordered by sequence ASC
    pub async fn list_by_conversation(
        &self,
        conversation_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>> {
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, conversation_id, role, content, artifacts,
                   model, input_tokens, output_tokens,
                   sequence, created_at
            FROM messages
            WHERE conversation_id = $1
            ORDER BY sequence ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(conversation_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// List all messages for a conversation (unpaginated, for LLM context)
    pub async fn list_all_by_conversation(&self, conversation_id: Uuid) -> Result<Vec<Message>> {
        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, conversation_id, role, content, artifacts,
                   model, input_tokens, output_tokens,
                   sequence, created_at
            FROM messages
            WHERE conversation_id = $1
            ORDER BY sequence ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Create a new message with an atomically assigned sequence number.
    ///
    /// The sequence is computed via a subquery (`COALESCE(MAX(sequence), 0) + 1`)
    /// in the INSERT itself, eliminating the race between reading and writing.
    pub async fn create(&self, msg: &Message) -> Result<Message> {
        let created = sqlx::query_as::<_, Message>(
            r#"
            INSERT INTO messages (
                id, conversation_id, role, content, artifacts,
                model, input_tokens, output_tokens,
                sequence, created_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                (SELECT COALESCE(MAX(sequence), 0) + 1
                 FROM messages
                 WHERE conversation_id = $2),
                $9
            )
            RETURNING id, conversation_id, role, content, artifacts,
                      model, input_tokens, output_tokens,
                      sequence, created_at
            "#,
        )
        .bind(msg.id)
        .bind(msg.conversation_id)
        .bind(msg.role)
        .bind(&msg.content)
        .bind(&msg.artifacts)
        .bind(&msg.model)
        .bind(msg.input_tokens)
        .bind(msg.output_tokens)
        .bind(msg.created_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(created)
    }

    /// Update the artifacts JSONB field on a message
    pub async fn update_artifacts(
        &self,
        message_id: Uuid,
        artifacts: serde_json::Value,
    ) -> Result<()> {
        sqlx::query("UPDATE messages SET artifacts = $2 WHERE id = $1")
            .bind(message_id)
            .bind(artifacts)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
