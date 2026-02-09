//! Conversations domain: LLM chat threads, messages

pub mod api;
pub mod domain;
pub mod repository;

// Re-export domain types at the crate root for convenience
pub use domain::entities::{Conversation, ConversationStatus, Message, MessageRole};
pub use domain::state::{
    ConversationEvent, ConversationState, ConversationStateMachine, StateError,
};

// Re-export repository types
pub use repository::{ConversationRepository, ConversationsRepositories, MessageRepository};

// Re-export API types
pub use api::routes;
pub use api::ConversationsState;
