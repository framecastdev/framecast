//! Configuration management following 12-factor app principles
//!
//! All configuration is loaded from environment variables to ensure
//! clean separation between code and config.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Database connection URL (Supabase PostgreSQL)
    pub database_url: String,

    /// Supabase configuration
    pub supabase_url: String,
    pub supabase_anon_key: String,
    pub supabase_service_role_key: String,

    /// External service APIs
    pub anthropic_api_key: String,
    pub inngest_event_key: String,
    pub inngest_signing_key: String,
    pub runpod_api_key: String,
    pub runpod_endpoint_id: String,

    /// Object storage
    pub s3_bucket_outputs: String,
    pub s3_bucket_assets: String,
    pub aws_region: String,

    /// Runtime configuration
    pub log_level: String,
    pub rust_log: String,
    pub port: u16,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok(); // Load .env file if it exists

        let config = Self {
            database_url: env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?,

            supabase_url: env::var("SUPABASE_URL")
                .map_err(|_| anyhow::anyhow!("SUPABASE_URL is required"))?,
            supabase_anon_key: env::var("SUPABASE_ANON_KEY")
                .map_err(|_| anyhow::anyhow!("SUPABASE_ANON_KEY is required"))?,
            supabase_service_role_key: env::var("SUPABASE_SERVICE_ROLE_KEY")
                .map_err(|_| anyhow::anyhow!("SUPABASE_SERVICE_ROLE_KEY is required"))?,

            anthropic_api_key: env::var("ANTHROPIC_API_KEY")
                .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY is required"))?,
            inngest_event_key: env::var("INNGEST_EVENT_KEY")
                .map_err(|_| anyhow::anyhow!("INNGEST_EVENT_KEY is required"))?,
            inngest_signing_key: env::var("INNGEST_SIGNING_KEY")
                .map_err(|_| anyhow::anyhow!("INNGEST_SIGNING_KEY is required"))?,
            runpod_api_key: env::var("RUNPOD_API_KEY")
                .map_err(|_| anyhow::anyhow!("RUNPOD_API_KEY is required"))?,
            runpod_endpoint_id: env::var("RUNPOD_ENDPOINT_ID")
                .map_err(|_| anyhow::anyhow!("RUNPOD_ENDPOINT_ID is required"))?,

            s3_bucket_outputs: env::var("S3_BUCKET_OUTPUTS")
                .map_err(|_| anyhow::anyhow!("S3_BUCKET_OUTPUTS is required"))?,
            s3_bucket_assets: env::var("S3_BUCKET_ASSETS")
                .map_err(|_| anyhow::anyhow!("S3_BUCKET_ASSETS is required"))?,
            aws_region: env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),

            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "framecast=debug".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
        };

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires .env file with all config vars - run locally only
    fn test_config_from_env_loads_successfully() {
        // Test that configuration loads successfully in development environment
        let result = Config::from_env();
        assert!(
            result.is_ok(),
            "Config should load successfully in development environment: {}",
            result
                .err()
                .map_or("Unknown error".to_string(), |e| e.to_string())
        );

        // Verify that required fields are populated
        let config = result.unwrap();
        assert!(
            !config.database_url.is_empty(),
            "DATABASE_URL should be populated"
        );
        assert!(
            !config.supabase_url.is_empty(),
            "SUPABASE_URL should be populated"
        );
        assert!(config.port > 0, "PORT should be a valid port number");
    }
}
