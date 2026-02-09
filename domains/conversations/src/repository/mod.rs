//! Repository implementations for Conversations domain

pub mod conversations;
pub mod messages;

use sqlx::PgPool;

pub use conversations::ConversationRepository;
pub use messages::MessageRepository;

/// Combined repository access for the Conversations domain
#[derive(Clone)]
pub struct ConversationsRepositories {
    pool: PgPool,
    pub conversations: ConversationRepository,
    pub messages: MessageRepository,
}

impl ConversationsRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            conversations: ConversationRepository::new(pool.clone()),
            messages: MessageRepository::new(pool.clone()),
            pool,
        }
    }

    /// Get a reference to the underlying pool (for cross-domain transactions)
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
