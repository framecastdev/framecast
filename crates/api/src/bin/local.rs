// Framecast API - Local Development Server
// Entry point for running the API locally during development

use std::net::SocketAddr;
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};

use framecast_common::config::Config;
use sqlx::PgPool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for structured logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .init();

    info!("Starting Framecast API local development server");

    // Load configuration from environment variables
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");

    // Create database connection pool
    let pool = PgPool::connect(&config.database_url).await.map_err(|e| {
        error!("Failed to connect to database: {}", e);
        anyhow::anyhow!("Database connection failed: {}", e)
    })?;

    info!("Database connection established");

    // Create the application router
    let app = framecast_api::create_app(config.clone(), pool)
        .await
        .map_err(|e| {
            error!("Failed to create application: {}", e);
            e
        })?;

    // Add development-specific middleware
    let app = app.layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(CorsLayer::permissive()) // Allow all origins in development
            .into_inner(),
    );

    // Create socket address from config
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));

    info!("Server starting on http://{}", addr);
    info!("Health check available at http://{}/health", addr);
    info!("API documentation available at http://{}/docs", addr);

    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("Server listening on {}", addr);

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal, starting graceful shutdown");
        },
        _ = terminate => {
            info!("Received terminate signal, starting graceful shutdown");
        },
    }
}
