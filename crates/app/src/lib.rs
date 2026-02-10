//! Framecast application composition root
//!
//! Composes all domain routers into a single application.

use axum::{
    extract::DefaultBodyLimit,
    http::{self, HeaderValue},
    Router,
};
use framecast_artifacts::{ArtifactsRepositories, ArtifactsState};
use framecast_auth::{AuthBackend, AuthConfig};
use framecast_conversations::{ConversationsRepositories, ConversationsState};
use framecast_email::{EmailConfig, EmailServiceFactory};
use framecast_inngest::{InngestConfig, InngestServiceFactory};
use framecast_jobs::{JobsRepositories, JobsState};
use framecast_llm::{LlmConfig, LlmServiceFactory};
use framecast_runpod::{RenderConfig, RenderServiceFactory};
use framecast_teams::{TeamsRepositories, TeamsState};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Create the main application router with all routes and middleware
pub async fn create_app(pool: PgPool) -> Result<Router, anyhow::Error> {
    // Create repositories
    let teams_repos = TeamsRepositories::new(pool.clone());
    let artifacts_repos = ArtifactsRepositories::new(pool.clone());
    let conversations_repos = ConversationsRepositories::new(pool.clone());
    let jobs_repos = JobsRepositories::new(pool.clone());

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

    // Create Inngest service from environment
    let inngest_config = InngestConfig::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create Inngest config: {}", e))?;
    let inngest_service = InngestServiceFactory::create(inngest_config)
        .map_err(|e| anyhow::anyhow!("Failed to create Inngest service: {}", e))?;

    // Create Render service from environment
    let render_config = RenderConfig::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create Render config: {}", e))?;

    let callback_base_url = render_config.callback_base_url.clone();

    // Build render service — extract mock behavior/history when using mock provider
    #[cfg(feature = "mock-render")]
    let (render_service_boxed, mock_render_behavior, mock_render_history) =
        if render_config.provider == "mock" {
            let mock = framecast_runpod::mock::MockRenderService::new(
                render_config.callback_base_url.clone(),
            );
            let behavior = Some(mock.behavior().clone());
            let history = Some(mock.history().clone());
            (
                Box::new(mock) as Box<dyn framecast_runpod::RenderService>,
                behavior,
                history,
            )
        } else {
            let svc = RenderServiceFactory::create(render_config)
                .map_err(|e| anyhow::anyhow!("Failed to create Render service: {}", e))?;
            (svc, None, None)
        };

    #[cfg(not(feature = "mock-render"))]
    let render_service_boxed = RenderServiceFactory::create(render_config)
        .map_err(|e| anyhow::anyhow!("Failed to create Render service: {}", e))?;

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

    // Create Jobs domain state
    let jobs_state = JobsState {
        repos: jobs_repos,
        auth: auth_backend.clone(),
        inngest: Arc::from(inngest_service),
        render: Arc::from(render_service_boxed),
        callback_base_url,
        #[cfg(feature = "mock-render")]
        mock_render_behavior,
        #[cfg(feature = "mock-render")]
        mock_render_history,
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
        .merge(framecast_jobs::routes().with_state(jobs_state))
        .merge(framecast_conversations::routes().with_state(conversations_state));

    Ok(app)
}

/// Build a CORS layer from a comma-separated origins string.
///
/// Each origin is trimmed and parsed into an [`AllowOrigin`] list.
/// Standard methods (GET, POST, PUT, PATCH, DELETE, OPTIONS) and
/// common API headers are permitted.
pub fn build_cors_layer(origins: &str) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed))
        .allow_methods([
            http::Method::GET,
            http::Method::POST,
            http::Method::PUT,
            http::Method::PATCH,
            http::Method::DELETE,
            http::Method::OPTIONS,
        ])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::HeaderName::from_static("x-api-key"),
        ])
        .max_age(std::time::Duration::from_secs(3600))
}

/// Maximum request body size (5 MiB).
const MAX_BODY_SIZE: usize = 5 * 1024 * 1024;

/// Returns a [`DefaultBodyLimit`] layer capping request bodies at [`MAX_BODY_SIZE`].
pub fn body_limit_layer() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_BODY_SIZE)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
