//! Framecast API - AWS Lambda Runtime

use lambda_http::{run, Error};
use sqlx::PgPool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use framecast_app::create_app;

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

    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    info!("Framecast API Lambda ready to serve requests");

    run(app).await
}
