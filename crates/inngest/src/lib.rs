//! Framecast Inngest Service
//!
//! Provides event-driven workflow orchestration with support for:
//! - Inngest HTTP event API integration for production
//! - Mock Inngest service for testing and development
//! - Configurable event key, base URL, and signing key

pub mod client;
pub mod mock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InngestError {
    #[error("Inngest configuration error: {0}")]
    Configuration(String),

    #[error("Inngest request error: {0}")]
    Request(String),

    #[error("Inngest response error: {0}")]
    Response(String),
}

/// An event to send to Inngest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InngestEvent {
    pub name: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
}

/// Inngest service configuration.
#[derive(Clone)]
pub struct InngestConfig {
    /// Inngest provider (inngest, mock)
    pub provider: String,
    /// Event key for authenticating with the Inngest event API
    pub event_key: String,
    /// Base URL for the Inngest event API
    pub base_url: String,
    /// Optional signing key for webhook verification
    pub signing_key: Option<String>,
}

impl std::fmt::Debug for InngestConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InngestConfig")
            .field("provider", &self.provider)
            .field("event_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field(
                "signing_key",
                &self.signing_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl InngestConfig {
    /// Create Inngest config from environment variables.
    pub fn from_env() -> Result<Self, InngestError> {
        let provider = std::env::var("INNGEST_PROVIDER").unwrap_or_else(|_| "mock".to_string());

        let event_key = std::env::var("INNGEST_EVENT_KEY").unwrap_or_else(|_| {
            if provider == "mock" {
                "mock-event-key".to_string()
            } else {
                String::new()
            }
        });

        let base_url = std::env::var("INNGEST_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8288".to_string());

        let signing_key = std::env::var("INNGEST_SIGNING_KEY").ok();

        if provider != "mock" && event_key.is_empty() {
            return Err(InngestError::Configuration(
                "INNGEST_EVENT_KEY is required for Inngest provider".to_string(),
            ));
        }

        Ok(Self {
            provider,
            event_key,
            base_url,
            signing_key,
        })
    }
}

/// Inngest service trait for different implementations.
#[async_trait::async_trait]
pub trait InngestService: Send + Sync {
    /// Send a single event to Inngest.
    async fn send_event(&self, event: InngestEvent) -> Result<(), InngestError>;

    /// Send multiple events to Inngest in a single request.
    async fn send_events(&self, events: Vec<InngestEvent>) -> Result<(), InngestError>;
}

/// Factory for creating InngestService implementations.
pub struct InngestServiceFactory;

impl InngestServiceFactory {
    /// Create an InngestService based on configuration.
    pub fn create(config: InngestConfig) -> Result<Box<dyn InngestService>, InngestError> {
        match config.provider.as_str() {
            "inngest" => {
                tracing::info!("Creating Inngest client service");
                if config.event_key.is_empty() {
                    return Err(InngestError::Configuration(
                        "INNGEST_EVENT_KEY is required for Inngest provider".to_string(),
                    ));
                }
                Ok(Box::new(client::InngestClient::new(config)))
            }
            "mock" => {
                tracing::info!("Creating mock Inngest service");
                Ok(Box::new(mock::MockInngestService::new()))
            }
            provider => Err(InngestError::Configuration(format!(
                "Unknown Inngest provider: {}. Supported providers: inngest, mock",
                provider
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // INN-U01: InngestConfig with valid inngest provider fields
    #[test]
    fn test_config_valid_inngest_provider() {
        let config = InngestConfig {
            provider: "inngest".to_string(),
            event_key: "test-key-123".to_string(),
            base_url: "http://localhost:9999".to_string(),
            signing_key: Some("sign-key-abc".to_string()),
        };
        assert_eq!(config.provider, "inngest");
        assert_eq!(config.event_key, "test-key-123");
        assert_eq!(config.base_url, "http://localhost:9999");
        assert_eq!(config.signing_key.as_deref(), Some("sign-key-abc"));
    }

    // INN-U02: InngestServiceFactory rejects inngest provider with empty event key
    #[test]
    fn test_factory_rejects_inngest_without_event_key() {
        let config = InngestConfig {
            provider: "inngest".to_string(),
            event_key: String::new(),
            base_url: "http://localhost:8288".to_string(),
            signing_key: None,
        };
        let result = InngestServiceFactory::create(config);
        assert!(result.is_err());
    }

    // INN-U03: InngestEvent serialization with all fields present
    #[test]
    fn test_event_serialization_all_fields() {
        let event = InngestEvent {
            name: "app/job.created".to_string(),
            data: serde_json::json!({"job_id": "123"}),
            user: Some(serde_json::json!({"user_id": "u456"})),
            id: Some("evt-789".to_string()),
            ts: Some(1700000000),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["name"], "app/job.created");
        assert_eq!(json["data"]["job_id"], "123");
        assert_eq!(json["user"]["user_id"], "u456");
        assert_eq!(json["id"], "evt-789");
        assert_eq!(json["ts"], 1700000000);
    }

    // INN-U04: InngestEvent serialization with optional fields omitted
    #[test]
    fn test_event_serialization_optional_fields_omitted() {
        let event = InngestEvent {
            name: "app/job.created".to_string(),
            data: serde_json::json!({"job_id": "123"}),
            user: None,
            id: None,
            ts: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"data\""));
        assert!(!json.contains("\"user\""));
        assert!(!json.contains("\"id\""));
        assert!(!json.contains("\"ts\""));
    }

    // INN-U05: InngestServiceFactory::create("mock") -> Ok
    #[test]
    fn test_factory_mock_succeeds() {
        let config = InngestConfig {
            provider: "mock".to_string(),
            event_key: String::new(),
            base_url: "http://localhost:8288".to_string(),
            signing_key: None,
        };
        let result = InngestServiceFactory::create(config);
        assert!(result.is_ok());
    }

    // INN-U06: InngestServiceFactory::create("inngest") with valid config -> Ok
    #[test]
    fn test_factory_inngest_succeeds() {
        let config = InngestConfig {
            provider: "inngest".to_string(),
            event_key: "test-key".to_string(),
            base_url: "http://localhost:8288".to_string(),
            signing_key: None,
        };
        let result = InngestServiceFactory::create(config);
        assert!(result.is_ok());
    }

    // INN-U07: InngestServiceFactory::create("invalid") -> Err
    #[test]
    fn test_factory_unknown_provider() {
        let config = InngestConfig {
            provider: "invalid".to_string(),
            event_key: "key".to_string(),
            base_url: "http://localhost:8288".to_string(),
            signing_key: None,
        };
        let err = match InngestServiceFactory::create(config) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown provider"),
        };
        assert!(err
            .to_string()
            .contains("Unknown Inngest provider: invalid"));
    }

    // INN-U08: MockInngestService::send_event() -> Ok, event stored
    #[tokio::test]
    async fn test_mock_send_event() {
        let service = mock::MockInngestService::new();

        let event = InngestEvent {
            name: "app/test.event".to_string(),
            data: serde_json::json!({"key": "value"}),
            user: None,
            id: None,
            ts: None,
        };

        service.send_event(event).await.unwrap();

        let recorded = service.recorded_events();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "app/test.event");
        assert_eq!(recorded[0].data["key"], "value");
    }

    // INN-U09: MockInngestService::send_events() -> Ok, all events stored
    #[tokio::test]
    async fn test_mock_send_events() {
        let service = mock::MockInngestService::new();

        let events = vec![
            InngestEvent {
                name: "app/event.one".to_string(),
                data: serde_json::json!({"idx": 1}),
                user: None,
                id: None,
                ts: None,
            },
            InngestEvent {
                name: "app/event.two".to_string(),
                data: serde_json::json!({"idx": 2}),
                user: None,
                id: None,
                ts: None,
            },
            InngestEvent {
                name: "app/event.three".to_string(),
                data: serde_json::json!({"idx": 3}),
                user: None,
                id: None,
                ts: None,
            },
        ];

        service.send_events(events).await.unwrap();

        let recorded = service.recorded_events();
        assert_eq!(recorded.len(), 3);
        assert_eq!(recorded[0].name, "app/event.one");
        assert_eq!(recorded[1].name, "app/event.two");
        assert_eq!(recorded[2].name, "app/event.three");
    }

    // INN-U10: InngestError variants have correct Display output
    #[test]
    fn test_error_display() {
        let config_err = InngestError::Configuration("bad config".to_string());
        assert_eq!(
            config_err.to_string(),
            "Inngest configuration error: bad config"
        );

        let request_err = InngestError::Request("connection refused".to_string());
        assert_eq!(
            request_err.to_string(),
            "Inngest request error: connection refused"
        );

        let response_err = InngestError::Response("500 Internal Server Error".to_string());
        assert_eq!(
            response_err.to_string(),
            "Inngest response error: 500 Internal Server Error"
        );
    }
}
