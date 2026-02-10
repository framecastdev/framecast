//! Framecast API - AWS Lambda Runtime

use lambda_http::{run, Error};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::info;

use framecast_app::{body_limit_layer, build_cors_layer, create_app};

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .without_time()
        .init();

    info!("Initializing Framecast API Lambda");

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| Error::from("DATABASE_URL environment variable is required"))?;

    let pool = PgPool::connect(&database_url)
        .await
        .map_err(|e| Error::from(format!("Database error: {}", e)))?;

    info!("Database connection established");

    let app = create_app(pool)
        .await
        .map_err(|e| Error::from(format!("App initialization error: {}", e)))?;

    let cors_origins = std::env::var("CORS_ALLOWED_ORIGINS")
        .map_err(|_| Error::from("CORS_ALLOWED_ORIGINS environment variable is required"))?;

    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&cors_origins))
        .layer(body_limit_layer());

    info!("Framecast API Lambda ready to serve requests");

    run(app).await
}
