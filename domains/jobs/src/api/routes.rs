//! Route definitions for Jobs domain API

use axum::{
    routing::{get, post},
    Router,
};

use super::handlers::{callbacks, jobs};
use super::middleware::JobsState;

/// Create all Jobs domain API routes
pub fn routes() -> Router<JobsState> {
    let router = Router::new()
        .route("/v1/jobs", get(jobs::list_jobs))
        .route("/v1/jobs/{id}", get(jobs::get_job).delete(jobs::delete_job))
        .route("/v1/jobs/{id}/cancel", post(jobs::cancel_job))
        .route("/v1/jobs/{id}/clone", post(jobs::clone_job))
        .route("/v1/jobs/{id}/events", get(jobs::get_job_events))
        .route("/v1/generate", post(jobs::create_ephemeral_job))
        .route("/v1/artifacts/{id}/render", post(jobs::render_artifact))
        .route("/internal/jobs/callback", post(callbacks::job_callback));

    #[cfg(feature = "mock-render")]
    let router = {
        use super::handlers::mock_admin;
        router
            .route(
                "/internal/mock/render/configure",
                post(mock_admin::configure_mock),
            )
            .route(
                "/internal/mock/render/history",
                get(mock_admin::get_history),
            )
            .route("/internal/mock/render/reset", post(mock_admin::reset_mock))
    };

    router
}
