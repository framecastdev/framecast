// Framecast API - AWS Lambda Runtime
// Entry point for deploying the API as AWS Lambda functions

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize tracing for structured logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    info!("Starting Framecast API Lambda function");

    // Basic Lambda handler - will be enhanced when API routes are implemented
    run(service_fn(
        |_event: LambdaEvent<serde_json::Value>| async move {
            info!("Received Lambda event");
            Ok::<serde_json::Value, Error>(serde_json::json!({
                "statusCode": 200,
                "body": "Framecast API v0.0.1-SNAPSHOT - Lambda deployment coming soon"
            }))
        },
    ))
    .await
}
