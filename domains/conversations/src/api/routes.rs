//! Route definitions for Conversations domain API

use axum::{routing::get, Router};

use super::handlers::{conversations, messages};
use super::middleware::ConversationsState;

/// Create conversation routes
fn conversation_routes() -> Router<ConversationsState> {
    Router::new()
        .route(
            "/v1/conversations",
            get(conversations::list_conversations).post(conversations::create_conversation),
        )
        .route(
            "/v1/conversations/{id}",
            get(conversations::get_conversation)
                .patch(conversations::update_conversation)
                .delete(conversations::delete_conversation),
        )
}

/// Create message routes
fn message_routes() -> Router<ConversationsState> {
    Router::new().route(
        "/v1/conversations/{conversation_id}/messages",
        get(messages::list_messages).post(messages::send_message),
    )
}

/// Create all Conversations domain API routes
pub fn routes() -> Router<ConversationsState> {
    Router::new()
        .merge(conversation_routes())
        .merge(message_routes())
}
