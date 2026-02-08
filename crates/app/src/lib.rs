//! Framecast application composition root
//!
//! Composes all domain routers into a single application.

use axum::Router;
use framecast_email::{EmailConfig, EmailServiceFactory};
use framecast_teams::{AuthConfig, TeamsRepositories, TeamsState};
use sqlx::PgPool;
use std::sync::Arc;

/// Create the main application router with all routes and middleware
pub async fn create_app(pool: PgPool) -> Result<Router, anyhow::Error> {
    // Create repositories
    let teams_repos = TeamsRepositories::new(pool);

    // Create auth config from environment
    let auth_config = AuthConfig {
        jwt_secret: std::env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET environment variable is required"))?,
        issuer: std::env::var("JWT_ISSUER").ok(),
        audience: std::env::var("JWT_AUDIENCE").ok(),
    };

    // Create email service from environment
    let email_config = EmailConfig::from_env()?;
    let email_service = EmailServiceFactory::create(email_config).await?;

    // Create Teams domain state
    let teams_state = TeamsState {
        repos: teams_repos,
        auth_config,
        email: Arc::from(email_service),
    };

    // Build router — compose domain routers with shared infrastructure routes
    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route(
            "/",
            axum::routing::get(|| async { "Framecast API v0.0.1-SNAPSHOT" }),
        )
        .merge(framecast_teams::routes().with_state(teams_state));

    Ok(app)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
