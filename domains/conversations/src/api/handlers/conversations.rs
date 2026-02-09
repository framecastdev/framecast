//! Conversation management API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AuthUser;
use framecast_common::{Error, Result, ValidatedJson};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::ConversationsState;
use crate::domain::entities::ConversationStatus;

/// Request for creating a conversation
#[derive(Debug, Deserialize, Validate)]
pub struct CreateConversationRequest {
    /// LLM model to use
    pub model: String,

    /// Optional conversation title
    pub title: Option<String>,

    /// Optional system prompt
    pub system_prompt: Option<String>,
}

/// Request for updating a conversation
#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub status: Option<ConversationStatus>,
}

/// Query params for listing conversations
#[derive(Debug, Deserialize)]
pub struct ListConversationsQuery {
    pub status: Option<ConversationStatus>,
}

/// Conversation response DTO
#[derive(Debug, Serialize)]
pub struct ConversationResponse {
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

impl From<crate::domain::entities::Conversation> for ConversationResponse {
    fn from(c: crate::domain::entities::Conversation) -> Self {
        Self {
            id: c.id,
            user_id: c.user_id,
            title: c.title,
            model: c.model,
            system_prompt: c.system_prompt,
            status: c.status,
            message_count: c.message_count,
            last_message_at: c.last_message_at,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// Create a new conversation
pub async fn create_conversation(
    AuthUser(ctx): AuthUser,
    State(state): State<ConversationsState>,
    ValidatedJson(req): ValidatedJson<CreateConversationRequest>,
) -> Result<(StatusCode, Json<ConversationResponse>)> {
    let conversation = crate::domain::entities::Conversation::new(
        ctx.user.id,
        req.model,
        req.title,
        req.system_prompt,
    )?;

    let created = state.repos.conversations.create(&conversation).await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// List conversations for the authenticated user
pub async fn list_conversations(
    AuthUser(ctx): AuthUser,
    State(state): State<ConversationsState>,
    Query(query): Query<ListConversationsQuery>,
) -> Result<Json<Vec<ConversationResponse>>> {
    let convs = state
        .repos
        .conversations
        .list_by_user(ctx.user.id, query.status)
        .await?;

    let responses: Vec<ConversationResponse> = convs.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Get a single conversation by ID
pub async fn get_conversation(
    AuthUser(ctx): AuthUser,
    State(state): State<ConversationsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ConversationResponse>> {
    let conv = state
        .repos
        .conversations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Conversation not found".to_string()))?;

    if conv.user_id != ctx.user.id {
        return Err(Error::NotFound("Conversation not found".to_string()));
    }

    Ok(Json(conv.into()))
}

/// Update a conversation (title, status)
pub async fn update_conversation(
    AuthUser(ctx): AuthUser,
    State(state): State<ConversationsState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<Json<ConversationResponse>> {
    // Verify ownership
    let conv = state
        .repos
        .conversations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Conversation not found".to_string()))?;

    if conv.user_id != ctx.user.id {
        return Err(Error::NotFound("Conversation not found".to_string()));
    }

    // Validate status transition if status is being changed
    if let Some(new_status) = req.status {
        use crate::domain::state::{
            ConversationEvent, ConversationState, ConversationStateMachine,
        };

        let current_state = match conv.status {
            ConversationStatus::Active => ConversationState::Active,
            ConversationStatus::Archived => ConversationState::Archived,
        };

        let event = match new_status {
            ConversationStatus::Active => ConversationEvent::Unarchive,
            ConversationStatus::Archived => ConversationEvent::Archive,
        };

        ConversationStateMachine::transition(current_state, event)
            .map_err(|e| Error::Validation(e.to_string()))?;
    }

    // Build title update: Some(Some(title)) = set, Some(None) = clear, None = skip
    let title_update = if req.title.is_some() {
        Some(req.title)
    } else {
        None
    };

    let updated = state
        .repos
        .conversations
        .update(id, title_update, req.status)
        .await?
        .ok_or_else(|| Error::NotFound("Conversation not found".to_string()))?;

    Ok(Json(updated.into()))
}

/// Delete a conversation
pub async fn delete_conversation(
    AuthUser(ctx): AuthUser,
    State(state): State<ConversationsState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let conv = state
        .repos
        .conversations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Conversation not found".to_string()))?;

    if conv.user_id != ctx.user.id {
        return Err(Error::NotFound("Conversation not found".to_string()));
    }

    state.repos.conversations.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
