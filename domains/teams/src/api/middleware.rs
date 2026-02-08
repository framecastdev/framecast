//! Teams domain state and auth backend integration

use crate::TeamsRepositories;
use axum::extract::FromRef;
use framecast_auth::AuthBackend;
use framecast_email::EmailService;
use std::sync::Arc;

/// Application state for the Teams domain
#[derive(Clone)]
pub struct TeamsState {
    pub repos: TeamsRepositories,
    pub auth: AuthBackend,
    pub email: Arc<dyn EmailService>,
}

impl FromRef<TeamsState> for AuthBackend {
    fn from_ref(state: &TeamsState) -> Self {
        state.auth.clone()
    }
}
