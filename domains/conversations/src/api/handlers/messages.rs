//! Message API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AnyAuth;
use framecast_common::{Error, Result, ValidatedJson};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::ConversationsState;
use crate::domain::entities::{ConversationStatus, MessageRole};

/// Request for sending a message
#[derive(Debug, Deserialize, Validate)]
pub struct SendMessageRequest {
    /// Message content
    pub content: String,
}

/// Message response DTO
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub artifacts: Option<serde_json::Value>,
    pub model: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub sequence: i32,
    pub created_at: DateTime<Utc>,
}

impl From<crate::domain::entities::Message> for MessageResponse {
    fn from(m: crate::domain::entities::Message) -> Self {
        Self {
            id: m.id,
            conversation_id: m.conversation_id,
            role: m.role,
            content: m.content,
            artifacts: m.artifacts.map(|j| j.0),
            model: m.model,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            sequence: m.sequence,
            created_at: m.created_at,
        }
    }
}

/// Response for send message (includes both user and assistant messages)
#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub user_message: MessageResponse,
    pub assistant_message: MessageResponse,
}

/// Send a message to a conversation
pub async fn send_message(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ConversationsState>,
    Path(conversation_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<SendMessageRequest>,
) -> Result<(StatusCode, Json<SendMessageResponse>)> {
    // Verify conversation exists and belongs to user
    let conv = state
        .repos
        .conversations
        .find(conversation_id)
        .await?
        .ok_or_else(|| Error::NotFound("Conversation not found".to_string()))?;

    if conv.user_id != ctx.user.id {
        return Err(Error::NotFound("Conversation not found".to_string()));
    }

    // Pre-condition: cannot send to archived conversation
    if conv.status == ConversationStatus::Archived {
        return Err(Error::Validation(
            "Cannot send messages to an archived conversation".to_string(),
        ));
    }

    // Get next sequence number
    let user_seq = state.repos.messages.next_sequence(conversation_id).await?;

    // Create user message
    let user_msg =
        crate::domain::entities::Message::new_user(conversation_id, req.content.clone(), user_seq)?;

    let created_user_msg = state.repos.messages.create(&user_msg).await?;

    // Build LLM request from conversation history
    let history = state
        .repos
        .messages
        .list_by_conversation(conversation_id)
        .await?;

    let llm_messages: Vec<framecast_llm::LlmMessage> = history
        .iter()
        .map(|m| framecast_llm::LlmMessage {
            role: match m.role {
                MessageRole::User => framecast_llm::LlmRole::User,
                MessageRole::Assistant => framecast_llm::LlmRole::Assistant,
            },
            content: m.content.clone(),
        })
        .collect();

    let llm_request = framecast_llm::CompletionRequest {
        model: conv.model.clone(),
        system_prompt: conv.system_prompt.clone(),
        messages: llm_messages,
        max_tokens: None,
    };

    // Call LLM
    let llm_response = state
        .llm
        .complete(llm_request)
        .await
        .map_err(|e| Error::Internal(format!("LLM error: {}", e)))?;

    // Create assistant message
    let assistant_seq = user_seq + 1;
    let assistant_msg = crate::domain::entities::Message::new_assistant(
        conversation_id,
        llm_response.content,
        assistant_seq,
        llm_response.model,
        llm_response.input_tokens,
        llm_response.output_tokens,
    )?;

    let created_assistant_msg = state.repos.messages.create(&assistant_msg).await?;

    // Update conversation stats (2 new messages)
    state
        .repos
        .conversations
        .update_message_stats(conversation_id, 2)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(SendMessageResponse {
            user_message: created_user_msg.into(),
            assistant_message: created_assistant_msg.into(),
        }),
    ))
}

/// List messages for a conversation
pub async fn list_messages(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ConversationsState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<Vec<MessageResponse>>> {
    // Verify conversation exists and belongs to user
    let conv = state
        .repos
        .conversations
        .find(conversation_id)
        .await?
        .ok_or_else(|| Error::NotFound("Conversation not found".to_string()))?;

    if conv.user_id != ctx.user.id {
        return Err(Error::NotFound("Conversation not found".to_string()));
    }

    let messages = state
        .repos
        .messages
        .list_by_conversation(conversation_id)
        .await?;

    let responses: Vec<MessageResponse> = messages.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}
