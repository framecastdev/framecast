//! Route definitions for Artifacts domain API

use axum::{routing::get, Router};

use super::handlers::{artifacts, system_assets};
use super::middleware::ArtifactsState;

/// Create artifact routes
fn artifact_routes() -> Router<ArtifactsState> {
    Router::new()
        .route("/v1/artifacts", get(artifacts::list_artifacts))
        .route(
            "/v1/artifacts/{id}",
            get(artifacts::get_artifact).delete(artifacts::delete_artifact),
        )
        .route(
            "/v1/artifacts/storyboards",
            axum::routing::post(artifacts::create_storyboard),
        )
}

/// Create system asset routes
fn system_asset_routes() -> Router<ArtifactsState> {
    Router::new()
        .route("/v1/system-assets", get(system_assets::list_system_assets))
        .route(
            "/v1/system-assets/{id}",
            get(system_assets::get_system_asset),
        )
}

/// Create all Artifacts domain API routes
pub fn routes() -> Router<ArtifactsState> {
    Router::new()
        .merge(artifact_routes())
        .merge(system_asset_routes())
}
