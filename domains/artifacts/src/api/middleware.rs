//! Artifacts domain state and auth backend integration

use crate::ArtifactsRepositories;
use axum::extract::FromRef;
use framecast_auth::AuthBackend;

/// Application state for the Artifacts domain
#[derive(Clone)]
pub struct ArtifactsState {
    pub repos: ArtifactsRepositories,
    pub auth: AuthBackend,
}

impl FromRef<ArtifactsState> for AuthBackend {
    fn from_ref(state: &ArtifactsState) -> Self {
        state.auth.clone()
    }
}
