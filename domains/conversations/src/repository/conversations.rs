//! Conversation repository

use crate::domain::entities::{Conversation, ConversationStatus};
use framecast_common::Result;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ConversationRepository {
    pool: PgPool,
}

impl ConversationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find conversation by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<Conversation>> {
        let conv = sqlx::query_as::<_, Conversation>(
            r#"
            SELECT id, user_id, title, model, system_prompt,
                   status, message_count, last_message_at,
                   created_at, updated_at
            FROM conversations
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(conv)
    }

    /// List conversations for a user, optionally filtering by status
    pub async fn list_by_user(
        &self,
        user_id: Uuid,
        status: Option<ConversationStatus>,
    ) -> Result<Vec<Conversation>> {
        let convs = match status {
            Some(s) => {
                sqlx::query_as::<_, Conversation>(
                    r#"
                    SELECT id, user_id, title, model, system_prompt,
                           status, message_count, last_message_at,
                           created_at, updated_at
                    FROM conversations
                    WHERE user_id = $1 AND status = $2
                    ORDER BY last_message_at DESC NULLS LAST, created_at DESC
                    "#,
                )
                .bind(user_id)
                .bind(s)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Conversation>(
                    r#"
                    SELECT id, user_id, title, model, system_prompt,
                           status, message_count, last_message_at,
                           created_at, updated_at
                    FROM conversations
                    WHERE user_id = $1 AND status = 'active'
                    ORDER BY last_message_at DESC NULLS LAST, created_at DESC
                    "#,
                )
                .bind(user_id)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(convs)
    }

    /// Create a new conversation
    pub async fn create(&self, conv: &Conversation) -> Result<Conversation> {
        let created = sqlx::query_as::<_, Conversation>(
            r#"
            INSERT INTO conversations (
                id, user_id, title, model, system_prompt,
                status, message_count, last_message_at,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, user_id, title, model, system_prompt,
                      status, message_count, last_message_at,
                      created_at, updated_at
            "#,
        )
        .bind(conv.id)
        .bind(conv.user_id)
        .bind(&conv.title)
        .bind(&conv.model)
        .bind(&conv.system_prompt)
        .bind(conv.status)
        .bind(conv.message_count)
        .bind(conv.last_message_at)
        .bind(conv.created_at)
        .bind(conv.updated_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(created)
    }

    /// Update conversation title and/or status
    pub async fn update(
        &self,
        id: Uuid,
        title: Option<Option<String>>,
        status: Option<ConversationStatus>,
    ) -> Result<Option<Conversation>> {
        // Build dynamic update; we pass all fields and use COALESCE
        let updated = sqlx::query_as::<_, Conversation>(
            r#"
            UPDATE conversations SET
                title = CASE WHEN $2 THEN $3 ELSE title END,
                status = COALESCE($4, status),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, user_id, title, model, system_prompt,
                      status, message_count, last_message_at,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(title.is_some())
        .bind(title.flatten())
        .bind(status)
        .fetch_optional(&self.pool)
        .await?;

        Ok(updated)
    }

    /// Update message count and last_message_at after sending messages
    pub async fn update_message_stats(
        &self,
        id: Uuid,
        message_count_increment: i32,
    ) -> Result<Option<Conversation>> {
        let updated = sqlx::query_as::<_, Conversation>(
            r#"
            UPDATE conversations SET
                message_count = message_count + $2,
                last_message_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, user_id, title, model, system_prompt,
                      status, message_count, last_message_at,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(message_count_increment)
        .fetch_optional(&self.pool)
        .await?;

        Ok(updated)
    }

    /// Delete a conversation
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM conversations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
