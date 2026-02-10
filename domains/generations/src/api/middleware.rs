//! Generations domain state and auth backend integration

use crate::GenerationsRepositories;
use axum::extract::FromRef;
use framecast_auth::AuthBackend;
use framecast_inngest::InngestService;
use framecast_runpod::RenderService;
use std::sync::Arc;

/// Application state for the Generations domain
#[derive(Clone)]
pub struct GenerationsState {
    pub repos: GenerationsRepositories,
    pub auth: AuthBackend,
    pub inngest: Arc<dyn InngestService>,
    pub render: Arc<dyn RenderService>,
    pub callback_base_url: String,
    #[cfg(feature = "mock-render")]
    pub mock_render_behavior: Option<Arc<framecast_runpod::mock::MockRenderBehavior>>,
    #[cfg(feature = "mock-render")]
    pub mock_render_history:
        Option<Arc<std::sync::Mutex<Vec<framecast_runpod::mock::RecordedRenderRequest>>>>,
}

impl FromRef<GenerationsState> for AuthBackend {
    fn from_ref(state: &GenerationsState) -> Self {
        state.auth.clone()
    }
}
