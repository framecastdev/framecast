//! Framecast API - AWS Lambda Runtime
//!
//! Entry point for deploying the API as an AWS Lambda function with API Gateway.
//! Uses lambda_http to integrate Axum router with Lambda runtime.

use lambda_http::{run, Error};
use sqlx::PgPool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use framecast_api::create_app;
use framecast_common::config::Config;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize tracing for structured logging (Lambda-compatible JSON format)
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .without_time() // Lambda adds timestamps
        .init();

    info!("Initializing Framecast API Lambda");

    // Load configuration from environment variables (12-Factor Rule III)
    let config = Config::from_env().map_err(|e| Error::from(format!("Config error: {}", e)))?;

    // Connect to database (12-Factor Rule IV: Backing Services)
    let pool = PgPool::connect(&config.database_url)
        .await
        .map_err(|e| Error::from(format!("Database error: {}", e)))?;

    info!("Database connection established");

    // Create Axum application with all routes
    let app = create_app(config, pool)
        .await
        .map_err(|e| Error::from(format!("App initialization error: {}", e)))?;

    // Add middleware layers
    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    info!("Framecast API Lambda ready to serve requests");

    // Run the Lambda runtime with the Axum app
    run(app).await
}
