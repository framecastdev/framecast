//! HTTP API handlers and Lambda runtime for Framecast

pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod validation;

use axum::Router;
use framecast_common::config::Config;
use framecast_db::repositories::Repositories;
use framecast_email::{EmailConfig, EmailServiceFactory};
use middleware::{AppState, AuthConfig};
use sqlx::PgPool;
use std::sync::Arc;

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

    // Create email service from environment
    let email_config = EmailConfig::from_env()?;
    let email_service = EmailServiceFactory::create(email_config).await?;

    // Create application state
    let state = AppState {
        repos,
        auth_config,
        email: Arc::from(email_service),
    };

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
