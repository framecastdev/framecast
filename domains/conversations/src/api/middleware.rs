//! Conversations domain state and auth backend integration

use crate::ConversationsRepositories;
use axum::extract::FromRef;
use framecast_auth::AuthBackend;
use framecast_llm::LlmService;
use std::sync::Arc;

/// Application state for the Conversations domain
#[derive(Clone)]
pub struct ConversationsState {
    pub repos: ConversationsRepositories,
    pub auth: AuthBackend,
    pub llm: Arc<dyn LlmService>,
}

impl FromRef<ConversationsState> for AuthBackend {
    fn from_ref(state: &ConversationsState) -> Self {
        state.auth.clone()
    }
}
