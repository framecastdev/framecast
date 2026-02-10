//! Framecast Render Service
//!
//! Provides render functionality for artifact rendering via GPU compute backends:
//! - RunPod GPU compute integration for production (planned)
//! - Mock render service for testing and development
//! - Configurable provider, callback URL, and programmable mock behavior

pub mod mock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Render configuration error: {0}")]
    Configuration(String),

    #[error("Render request error: {0}")]
    Request(String),

    #[error("Render response error: {0}")]
    Response(String),
}

/// Request to submit a render generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub generation_id: uuid::Uuid,
    pub spec_snapshot: serde_json::Value,
    pub options: serde_json::Value,
    pub callback_url: String,
}

/// Result from a render generation (sent via callback)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub generation_id: uuid::Uuid,
    pub status: String,
    pub output: Option<serde_json::Value>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<serde_json::Value>,
}

/// Render service configuration
#[derive(Clone)]
pub struct RenderConfig {
    pub provider: String,
    pub callback_base_url: String,
}

impl std::fmt::Debug for RenderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderConfig")
            .field("provider", &self.provider)
            .field("callback_base_url", &self.callback_base_url)
            .finish()
    }
}

impl RenderConfig {
    /// Create render config from environment variables
    pub fn from_env() -> Result<Self, RenderError> {
        let provider = std::env::var("RENDER_PROVIDER").unwrap_or_else(|_| "mock".to_string());
        let callback_base_url = std::env::var("RENDER_CALLBACK_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

        Ok(Self {
            provider,
            callback_base_url,
        })
    }
}

/// Render service trait for different compute backends
#[async_trait::async_trait]
pub trait RenderService: Send + Sync {
    /// Submit a render job to the compute backend.
    /// The backend will POST results to callback_url when done.
    async fn submit_render(&self, request: RenderRequest) -> Result<(), RenderError>;
}

/// Factory for creating RenderService implementations
pub struct RenderServiceFactory;

impl RenderServiceFactory {
    pub fn create(config: RenderConfig) -> Result<Box<dyn RenderService>, RenderError> {
        match config.provider.as_str() {
            "runpod" => {
                tracing::info!("Creating RunPod render service");
                // RunPod client will be implemented later when we integrate with the real API
                Err(RenderError::Configuration(
                    "RunPod provider not yet implemented. Use 'mock' provider.".to_string(),
                ))
            }
            "mock" => {
                tracing::info!("Creating mock render service");
                Ok(Box::new(mock::MockRenderService::new(
                    config.callback_base_url,
                )))
            }
            provider => Err(RenderError::Configuration(format!(
                "Unknown render provider: {}. Supported providers: runpod, mock",
                provider
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RP-U01: RenderConfig with default-like values
    #[test]
    fn test_render_config_defaults() {
        let config = RenderConfig {
            provider: "mock".to_string(),
            callback_base_url: "http://localhost:3000".to_string(),
        };
        assert_eq!(config.provider, "mock");
        assert_eq!(config.callback_base_url, "http://localhost:3000");
    }

    // RP-U02: RenderConfig with custom values
    #[test]
    fn test_render_config_custom() {
        let config = RenderConfig {
            provider: "runpod".to_string(),
            callback_base_url: "https://api.example.com".to_string(),
        };
        assert_eq!(config.provider, "runpod");
        assert_eq!(config.callback_base_url, "https://api.example.com");
    }

    // RP-U03: Factory creates mock provider successfully
    #[test]
    fn test_factory_mock_succeeds() {
        let config = RenderConfig {
            provider: "mock".to_string(),
            callback_base_url: "http://localhost:3000".to_string(),
        };
        let result = RenderServiceFactory::create(config);
        assert!(result.is_ok());
    }

    // RP-U04: Factory rejects unknown provider
    #[test]
    fn test_factory_unknown_provider() {
        let config = RenderConfig {
            provider: "invalid".to_string(),
            callback_base_url: "http://localhost:3000".to_string(),
        };
        let result = RenderServiceFactory::create(config);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        assert!(err.to_string().contains("Unknown render provider: invalid"));
    }

    // RP-U05: RenderRequest serialization round-trip
    #[test]
    fn test_render_request_serialization_round_trip() {
        let generation_id = uuid::Uuid::new_v4();
        let request = RenderRequest {
            generation_id,
            spec_snapshot: serde_json::json!({"frames": [{"prompt": "A sunset"}]}),
            options: serde_json::json!({"resolution": "1080p"}),
            callback_url: "http://localhost:3000/callbacks/render".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RenderRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.generation_id, generation_id);
        assert_eq!(deserialized.spec_snapshot, request.spec_snapshot);
        assert_eq!(deserialized.options, request.options);
        assert_eq!(deserialized.callback_url, request.callback_url);
    }

    // RP-U06: RenderResult serialization — success case
    #[test]
    fn test_render_result_serialization_success() {
        let generation_id = uuid::Uuid::new_v4();
        let result = RenderResult {
            generation_id,
            status: "completed".to_string(),
            output: Some(serde_json::json!({"url": "https://storage.example.com/video.mp4"})),
            output_size_bytes: Some(2048),
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: RenderResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.generation_id, generation_id);
        assert_eq!(deserialized.status, "completed");
        assert!(deserialized.output.is_some());
        assert_eq!(deserialized.output_size_bytes, Some(2048));
        assert!(deserialized.error.is_none());
    }

    // RP-U07: RenderResult serialization — failure case
    #[test]
    fn test_render_result_serialization_failure() {
        let generation_id = uuid::Uuid::new_v4();
        let result = RenderResult {
            generation_id,
            status: "failed".to_string(),
            output: None,
            output_size_bytes: None,
            error: Some(serde_json::json!({"message": "GPU out of memory", "code": "OOM"})),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: RenderResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.generation_id, generation_id);
        assert_eq!(deserialized.status, "failed");
        assert!(deserialized.output.is_none());
        assert!(deserialized.output_size_bytes.is_none());
        assert!(deserialized.error.is_some());
        let error = deserialized.error.unwrap();
        assert_eq!(error["message"], "GPU out of memory");
        assert_eq!(error["code"], "OOM");
    }

    // RP-U08: MockOutcome default is Complete
    #[test]
    fn test_mock_outcome_default_is_complete() {
        use crate::mock::*;

        let outcome = MockOutcome::default();
        assert_eq!(outcome, MockOutcome::Complete);
    }

    // RP-U09: MockRenderBehavior default delay is 50ms
    #[test]
    fn test_mock_render_behavior_default_delay() {
        use crate::mock::*;

        let behavior = MockRenderBehavior::new();
        assert_eq!(behavior.get_delay_ms(), 50);
        assert_eq!(behavior.get_outcome(), MockOutcome::Complete);
    }

    // RP-U10: MockRenderBehavior::configure() changes outcome
    #[test]
    fn test_mock_render_behavior_configure() {
        use crate::mock::*;

        let behavior = MockRenderBehavior::new();
        assert_eq!(behavior.get_outcome(), MockOutcome::Complete);

        behavior.set_outcome(MockOutcome::Fail);
        assert_eq!(behavior.get_outcome(), MockOutcome::Fail);

        behavior.set_outcome(MockOutcome::Timeout);
        assert_eq!(behavior.get_outcome(), MockOutcome::Timeout);

        behavior.set_delay_ms(500);
        assert_eq!(behavior.get_delay_ms(), 500);
    }

    // RP-U11: MockRenderBehavior::reset() restores defaults
    #[test]
    fn test_mock_render_behavior_reset() {
        use crate::mock::*;

        let behavior = MockRenderBehavior::new();

        // Change everything
        behavior.set_outcome(MockOutcome::Fail);
        behavior.set_delay_ms(1000);
        behavior.set_progress_steps(vec![25.0, 50.0, 75.0]);
        behavior.set_error_payload(serde_json::json!({"error": "test"}));
        behavior.set_output_payload(serde_json::json!({"output": "test"}));

        // Verify changed
        assert_eq!(behavior.get_outcome(), MockOutcome::Fail);
        assert_eq!(behavior.get_delay_ms(), 1000);

        // Reset
        behavior.reset();

        // Verify defaults restored
        assert_eq!(behavior.get_outcome(), MockOutcome::Complete);
        assert_eq!(behavior.get_delay_ms(), 50);
        assert!(behavior.progress_steps.read().unwrap().is_empty());
        assert!(behavior.error_payload.read().unwrap().is_none());
        assert!(behavior.output_payload.read().unwrap().is_none());
    }

    // RP-U12: RenderError variants have correct Display output
    #[test]
    fn test_render_error_display() {
        let config_err = RenderError::Configuration("missing key".to_string());
        assert_eq!(
            config_err.to_string(),
            "Render configuration error: missing key"
        );

        let request_err = RenderError::Request("timeout".to_string());
        assert_eq!(request_err.to_string(), "Render request error: timeout");

        let response_err = RenderError::Response("invalid json".to_string());
        assert_eq!(
            response_err.to_string(),
            "Render response error: invalid json"
        );
    }
}
