//! Framecast application composition root
//!
//! Composes all domain routers into a single application.

use axum::Router;
use framecast_artifacts::{ArtifactsRepositories, ArtifactsState};
use framecast_auth::{AuthBackend, AuthConfig};
use framecast_conversations::{ConversationsRepositories, ConversationsState};
use framecast_email::{EmailConfig, EmailServiceFactory};
use framecast_llm::{LlmConfig, LlmServiceFactory};
use framecast_teams::{TeamsRepositories, TeamsState};
use sqlx::PgPool;
use std::sync::Arc;

/// Create the main application router with all routes and middleware
pub async fn create_app(pool: PgPool) -> Result<Router, anyhow::Error> {
    // Create repositories
    let teams_repos = TeamsRepositories::new(pool.clone());
    let artifacts_repos = ArtifactsRepositories::new(pool.clone());
    let conversations_repos = ConversationsRepositories::new(pool.clone());

    // Create auth config from environment
    let auth_config = AuthConfig {
        jwt_secret: std::env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET environment variable is required"))?,
        issuer: std::env::var("JWT_ISSUER").ok(),
        audience: std::env::var("JWT_AUDIENCE").ok(),
    };

    // Create auth backend
    let auth_backend = AuthBackend::new(pool, auth_config);

    // Create email service from environment
    let email_config = EmailConfig::from_env()?;
    let email_service = EmailServiceFactory::create(email_config).await?;

    // Create LLM service from environment
    let llm_config = LlmConfig::from_env().unwrap_or_else(|_| LlmConfig {
        provider: "mock".to_string(),
        api_key: String::new(),
        default_model: "mock".to_string(),
        max_tokens: 4096,
        base_url: None,
    });
    let llm_service = LlmServiceFactory::create(llm_config)
        .map_err(|e| anyhow::anyhow!("Failed to create LLM service: {}", e))?;

    // Create Teams domain state
    let teams_state = TeamsState {
        repos: teams_repos,
        auth: auth_backend.clone(),
        email: Arc::from(email_service),
    };

    // Create Artifacts domain state
    let artifacts_state = ArtifactsState {
        repos: artifacts_repos,
        auth: auth_backend.clone(),
    };

    // Create Conversations domain state
    let conversations_state = ConversationsState {
        repos: conversations_repos,
        auth: auth_backend,
        llm: Arc::from(llm_service),
    };

    // Build router — compose domain routers with shared infrastructure routes
    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route(
            "/",
            axum::routing::get(|| async { "Framecast API v0.0.1-SNAPSHOT" }),
        )
        .merge(framecast_teams::routes().with_state(teams_state))
        .merge(framecast_artifacts::routes().with_state(artifacts_state))
        .merge(framecast_conversations::routes().with_state(conversations_state));

    Ok(app)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
