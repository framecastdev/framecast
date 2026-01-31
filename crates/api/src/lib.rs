//! HTTP API handlers and Lambda runtime for Framecast

pub mod handlers;
pub mod middleware;
pub mod routes;

use axum::Router;
use framecast_common::config::Config;

/// Create the main application router with all routes and middleware
pub async fn create_app(_config: Config) -> Result<Router, anyhow::Error> {
    // TODO: Implement proper application setup
    // For now, return a basic router to make the build pass
    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route(
            "/",
            axum::routing::get(|| async { "Framecast API v0.0.1-SNAPSHOT" }),
        );

    Ok(app)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
