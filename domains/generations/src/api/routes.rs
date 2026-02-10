//! Route definitions for Generations domain API

use axum::{
    routing::{get, post},
    Router,
};

use super::handlers::{callbacks, generations};
use super::middleware::GenerationsState;

/// Create all Generations domain API routes
pub fn routes() -> Router<GenerationsState> {
    let router = Router::new()
        .route(
            "/v1/generations",
            get(generations::list_generations).post(generations::create_generation),
        )
        .route(
            "/v1/generations/{id}",
            get(generations::get_generation).delete(generations::delete_generation),
        )
        .route(
            "/v1/generations/{id}/cancel",
            post(generations::cancel_generation),
        )
        .route(
            "/v1/generations/{id}/clone",
            post(generations::clone_generation),
        )
        .route(
            "/v1/generations/{id}/events",
            get(generations::get_generation_events),
        )
        .route(
            "/internal/generations/callback",
            post(callbacks::generation_callback),
        );

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
