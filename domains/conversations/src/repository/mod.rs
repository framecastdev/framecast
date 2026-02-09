//! Repository implementations for Conversations domain

pub mod conversations;
pub mod messages;

use sqlx::PgPool;

pub use conversations::ConversationRepository;
pub use messages::MessageRepository;

/// Combined repository access for the Conversations domain
#[derive(Clone)]
pub struct ConversationsRepositories {
    pub conversations: ConversationRepository,
    pub messages: MessageRepository,
}

impl ConversationsRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            conversations: ConversationRepository::new(pool.clone()),
            messages: MessageRepository::new(pool),
        }
    }
}
