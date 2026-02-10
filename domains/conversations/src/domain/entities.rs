//! Domain entities for Conversations domain
//!
//! This module contains conversation-related domain entities as defined in the API specification.
//! Each entity includes proper validation, serialization, and business rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use framecast_common::{Error, Result};

/// Conversation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "conversation_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ConversationStatus {
    #[default]
    Active,
    Archived,
}

impl std::fmt::Display for ConversationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversationStatus::Active => write!(f, "active"),
            ConversationStatus::Archived => write!(f, "archived"),
        }
    }
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "message_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
        }
    }
}

/// Maximum model string length (varchar(100))
const MAX_MODEL_LENGTH: usize = 100;

/// Maximum title string length (varchar(200))
const MAX_TITLE_LENGTH: usize = 200;

/// Maximum system prompt length (CHECK length <= 10000)
const MAX_SYSTEM_PROMPT_LENGTH: usize = 10000;

/// Conversation entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Conversation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: Option<String>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub status: ConversationStatus,
    pub message_count: i32,
    pub last_message_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    /// Create a new conversation
    pub fn new(
        user_id: Uuid,
        model: String,
        title: Option<String>,
        system_prompt: Option<String>,
    ) -> Result<Self> {
        // Validate model (required, varchar(100))
        if model.is_empty() {
            return Err(Error::Validation("Model is required".to_string()));
        }
        if model.len() > MAX_MODEL_LENGTH {
            return Err(Error::Validation(format!(
                "Model must be at most {} characters",
                MAX_MODEL_LENGTH
            )));
        }

        // Validate title (optional, varchar(200))
        if let Some(ref t) = title {
            if t.len() > MAX_TITLE_LENGTH {
                return Err(Error::Validation(format!(
                    "Title must be at most {} characters",
                    MAX_TITLE_LENGTH
                )));
            }
        }

        // Validate system_prompt (optional, CHECK length <= 10000)
        if let Some(ref sp) = system_prompt {
            if sp.len() > MAX_SYSTEM_PROMPT_LENGTH {
                return Err(Error::Validation(format!(
                    "System prompt must be at most {} characters",
                    MAX_SYSTEM_PROMPT_LENGTH
                )));
            }
        }

        let now = Utc::now();
        Ok(Conversation {
            id: Uuid::new_v4(),
            user_id,
            title,
            model,
            system_prompt,
            status: ConversationStatus::default(),
            message_count: 0,
            last_message_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Increment message count by a given amount (INV-C4)
    pub fn increment_message_count(&mut self, count: i32) -> Result<()> {
        if count < 1 {
            return Err(Error::Validation(
                "Message count increment must be at least 1".to_string(),
            ));
        }
        self.message_count = self
            .message_count
            .checked_add(count)
            .ok_or_else(|| Error::Validation("Message count overflow".to_string()))?;
        Ok(())
    }
}

/// Message entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub artifacts: Option<Json<serde_json::Value>>,
    pub model: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub sequence: i32,
    pub created_at: DateTime<Utc>,
}

impl Message {
    /// Create a new user message
    pub fn new_user(conversation_id: Uuid, content: String, sequence: i32) -> Result<Self> {
        Self::validate_content(&content)?;
        Self::validate_sequence(sequence)?;

        Ok(Message {
            id: Uuid::new_v4(),
            conversation_id,
            role: MessageRole::User,
            content,
            artifacts: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            sequence,
            created_at: Utc::now(),
        })
    }

    /// Create a new assistant message
    pub fn new_assistant(
        conversation_id: Uuid,
        content: String,
        sequence: i32,
        model: String,
        input_tokens: i32,
        output_tokens: i32,
    ) -> Result<Self> {
        Self::validate_content(&content)?;
        Self::validate_sequence(sequence)?;

        Ok(Message {
            id: Uuid::new_v4(),
            conversation_id,
            role: MessageRole::Assistant,
            content,
            artifacts: None,
            model: Some(model),
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            sequence,
            created_at: Utc::now(),
        })
    }

    /// Validate message content (CHECK (length(trim(content)) > 0))
    fn validate_content(content: &str) -> Result<()> {
        if content.trim().is_empty() {
            return Err(Error::Validation(
                "Message content cannot be empty or whitespace-only".to_string(),
            ));
        }
        Ok(())
    }

    /// Validate sequence (CHECK (sequence >= 1))
    fn validate_sequence(sequence: i32) -> Result<()> {
        if sequence < 1 {
            return Err(Error::Validation(
                "Message sequence must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

// ============================================================================
// Tests — 40 unit tests (CON-U01 through CON-U40)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // 1.1 Enum Tests (CON-U01 through CON-U05)

    #[test]
    fn test_conversation_status_display_active() {
        assert_eq!(ConversationStatus::Active.to_string(), "active");
    }

    #[test]
    fn test_conversation_status_display_archived() {
        assert_eq!(ConversationStatus::Archived.to_string(), "archived");
    }

    #[test]
    fn test_conversation_status_default_is_active() {
        assert_eq!(ConversationStatus::default(), ConversationStatus::Active);
    }

    #[test]
    fn test_message_role_display_user() {
        assert_eq!(MessageRole::User.to_string(), "user");
    }

    #[test]
    fn test_message_role_display_assistant() {
        assert_eq!(MessageRole::Assistant.to_string(), "assistant");
    }

    // 1.2 Conversation Entity (CON-U06 through CON-U17)

    #[test]
    fn test_conversation_creation_model_only() {
        let user_id = Uuid::new_v4();
        let conv = Conversation::new(
            user_id,
            "claude-sonnet-4-5-20250929".to_string(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(conv.user_id, user_id);
        assert_eq!(conv.model, "claude-sonnet-4-5-20250929");
        assert_eq!(conv.status, ConversationStatus::Active);
        assert_eq!(conv.message_count, 0);
        assert!(conv.last_message_at.is_none());
        assert!(conv.title.is_none());
        assert!(conv.system_prompt.is_none());
    }

    #[test]
    fn test_conversation_creation_all_fields() {
        let user_id = Uuid::new_v4();
        let conv = Conversation::new(
            user_id,
            "claude-sonnet-4-5-20250929".to_string(),
            Some("My Chat".to_string()),
            Some("You are a helpful assistant.".to_string()),
        )
        .unwrap();

        assert_eq!(conv.model, "claude-sonnet-4-5-20250929");
        assert_eq!(conv.title.as_deref(), Some("My Chat"));
        assert_eq!(
            conv.system_prompt.as_deref(),
            Some("You are a helpful assistant.")
        );
    }

    #[test]
    fn test_conversation_model_empty_rejected() {
        let result = Conversation::new(Uuid::new_v4(), "".to_string(), None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Model is required"));
    }

    #[test]
    fn test_conversation_model_100_chars_valid() {
        let model = "a".repeat(100);
        let result = Conversation::new(Uuid::new_v4(), model.clone(), None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().model, model);
    }

    #[test]
    fn test_conversation_model_101_chars_rejected() {
        let model = "a".repeat(101);
        let result = Conversation::new(Uuid::new_v4(), model, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at most 100"));
    }

    #[test]
    fn test_conversation_title_200_chars_valid() {
        let title = "a".repeat(200);
        let result = Conversation::new(
            Uuid::new_v4(),
            "model".to_string(),
            Some(title.clone()),
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().title.as_deref(), Some(title.as_str()));
    }

    #[test]
    fn test_conversation_title_201_chars_rejected() {
        let title = "a".repeat(201);
        let result = Conversation::new(Uuid::new_v4(), "model".to_string(), Some(title), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at most 200"));
    }

    #[test]
    fn test_conversation_title_none_valid() {
        let result = Conversation::new(Uuid::new_v4(), "model".to_string(), None, None);
        assert!(result.is_ok());
        assert!(result.unwrap().title.is_none());
    }

    #[test]
    fn test_conversation_system_prompt_10000_valid() {
        let prompt = "a".repeat(10000);
        let result = Conversation::new(
            Uuid::new_v4(),
            "model".to_string(),
            None,
            Some(prompt.clone()),
        );
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().system_prompt.as_deref(),
            Some(prompt.as_str())
        );
    }

    #[test]
    fn test_conversation_system_prompt_10001_rejected() {
        let prompt = "a".repeat(10001);
        let result = Conversation::new(Uuid::new_v4(), "model".to_string(), None, Some(prompt));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at most 10000"));
    }

    #[test]
    fn test_conversation_system_prompt_none_valid() {
        let result = Conversation::new(Uuid::new_v4(), "model".to_string(), None, None);
        assert!(result.is_ok());
        assert!(result.unwrap().system_prompt.is_none());
    }

    #[test]
    fn test_conversation_system_prompt_empty_valid() {
        let result = Conversation::new(
            Uuid::new_v4(),
            "model".to_string(),
            None,
            Some("".to_string()),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().system_prompt.as_deref(), Some(""));
    }

    // 1.4 Message Entity (CON-U24 through CON-U35)

    #[test]
    fn test_user_message_creation() {
        let conv_id = Uuid::new_v4();
        let msg = Message::new_user(conv_id, "Hello".to_string(), 1).unwrap();

        assert_eq!(msg.conversation_id, conv_id);
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.sequence, 1);
        assert!(msg.model.is_none());
        assert!(msg.input_tokens.is_none());
        assert!(msg.output_tokens.is_none());
    }

    #[test]
    fn test_assistant_message_creation() {
        let conv_id = Uuid::new_v4();
        let msg = Message::new_assistant(
            conv_id,
            "Reply".to_string(),
            2,
            "claude-sonnet-4-5-20250929".to_string(),
            100,
            50,
        )
        .unwrap();

        assert_eq!(msg.conversation_id, conv_id);
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "Reply");
        assert_eq!(msg.sequence, 2);
        assert_eq!(msg.model.as_deref(), Some("claude-sonnet-4-5-20250929"));
        assert_eq!(msg.input_tokens, Some(100));
        assert_eq!(msg.output_tokens, Some(50));
    }

    #[test]
    fn test_message_content_empty_rejected() {
        let result = Message::new_user(Uuid::new_v4(), "".to_string(), 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_message_content_whitespace_only_rejected() {
        let result = Message::new_user(Uuid::new_v4(), "   \t\n  ".to_string(), 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_message_content_single_char_valid() {
        let result = Message::new_user(Uuid::new_v4(), "x".to_string(), 1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "x");
    }

    #[test]
    fn test_message_content_with_surrounding_whitespace_valid() {
        let result = Message::new_user(Uuid::new_v4(), "  hello  ".to_string(), 1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "  hello  ");
    }

    #[test]
    fn test_message_sequence_zero_rejected() {
        let result = Message::new_user(Uuid::new_v4(), "hi".to_string(), 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[test]
    fn test_message_sequence_negative_rejected() {
        let result = Message::new_user(Uuid::new_v4(), "hi".to_string(), -1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[test]
    fn test_message_sequence_one_valid() {
        let result = Message::new_user(Uuid::new_v4(), "hi".to_string(), 1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sequence, 1);
    }

    #[test]
    fn test_message_sequence_large_value_valid() {
        let result = Message::new_user(Uuid::new_v4(), "hi".to_string(), 999999);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sequence, 999999);
    }

    #[test]
    fn test_message_artifacts_jsonb_none_valid() {
        let msg = Message::new_user(Uuid::new_v4(), "hi".to_string(), 1).unwrap();
        assert!(msg.artifacts.is_none());
    }

    #[test]
    fn test_message_artifacts_jsonb_present_valid() {
        let mut msg = Message::new_user(Uuid::new_v4(), "hi".to_string(), 1).unwrap();
        msg.artifacts = Some(Json(serde_json::json!([{"id": "abc"}])));
        assert!(msg.artifacts.is_some());
    }

    // 1.5 Serialization (CON-U36 through CON-U40)

    #[test]
    fn test_conversation_serialization_roundtrip() {
        let conv = Conversation::new(
            Uuid::new_v4(),
            "model".to_string(),
            Some("Test".to_string()),
            None,
        )
        .unwrap();

        let json = serde_json::to_string(&conv).unwrap();
        let deserialized: Conversation = serde_json::from_str(&json).unwrap();

        assert_eq!(conv.id, deserialized.id);
        assert_eq!(conv.model, deserialized.model);
        assert_eq!(conv.title, deserialized.title);
        assert_eq!(conv.status, deserialized.status);
        assert_eq!(conv.message_count, deserialized.message_count);
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::new_user(Uuid::new_v4(), "hello".to_string(), 1).unwrap();

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.role, deserialized.role);
        assert_eq!(msg.content, deserialized.content);
        assert_eq!(msg.sequence, deserialized.sequence);
    }

    #[test]
    fn test_conversation_status_serialization_lowercase() {
        let json = serde_json::to_string(&ConversationStatus::Active).unwrap();
        assert_eq!(json, "\"active\"");

        let json = serde_json::to_string(&ConversationStatus::Archived).unwrap();
        assert_eq!(json, "\"archived\"");
    }

    #[test]
    fn test_message_role_serialization_lowercase() {
        let json = serde_json::to_string(&MessageRole::User).unwrap();
        assert_eq!(json, "\"user\"");

        let json = serde_json::to_string(&MessageRole::Assistant).unwrap();
        assert_eq!(json, "\"assistant\"");
    }

    #[test]
    fn test_conversation_increment_message_count() {
        let mut conv = Conversation::new(Uuid::new_v4(), "model".to_string(), None, None).unwrap();
        assert_eq!(conv.message_count, 0);

        conv.increment_message_count(1).unwrap();
        assert_eq!(conv.message_count, 1);

        conv.increment_message_count(2).unwrap();
        assert_eq!(conv.message_count, 3);
    }

    #[test]
    fn test_conversation_increment_message_count_zero_rejected() {
        let mut conv = Conversation::new(Uuid::new_v4(), "model".to_string(), None, None).unwrap();
        let result = conv.increment_message_count(0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[test]
    fn test_conversation_increment_message_count_negative_rejected() {
        let mut conv = Conversation::new(Uuid::new_v4(), "model".to_string(), None, None).unwrap();
        let result = conv.increment_message_count(-1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[test]
    fn test_conversation_increment_message_count_overflow_rejected() {
        let mut conv = Conversation::new(Uuid::new_v4(), "model".to_string(), None, None).unwrap();
        conv.message_count = i32::MAX;
        let result = conv.increment_message_count(1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("overflow"));
    }
}
