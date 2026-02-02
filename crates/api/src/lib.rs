//! HTTP API handlers and Lambda runtime for Framecast

pub mod handlers;
pub mod middleware;
pub mod routes;

use axum::Router;
use framecast_common::config::Config;
use framecast_db::repositories::Repositories;
use middleware::{AppState, AuthConfig};
use sqlx::PgPool;

/// Create the main application router with all routes and middleware
pub async fn create_app(_config: Config, pool: PgPool) -> Result<Router, anyhow::Error> {
    // Create repositories
    let repos = Repositories::new(pool);

    // Create auth config from environment
    let auth_config = AuthConfig {
        jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret".to_string()),
        issuer: std::env::var("JWT_ISSUER").ok(),
        audience: std::env::var("JWT_AUDIENCE").ok(),
    };

    // Create application state
    let state = AppState { repos, auth_config };

    // Build router with all routes
    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route(
            "/",
            axum::routing::get(|| async { "Framecast API v0.0.1-SNAPSHOT" }),
        )
        .merge(routes::create_routes())
        .with_state(state);

    Ok(app)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
