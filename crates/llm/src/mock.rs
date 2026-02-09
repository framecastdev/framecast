//! Mock LLM Service Implementations
//!
//! Two mock implementations for testing:
//! - `MockLlmService`: stateless echo mock used by `LlmServiceFactory`
//! - `ConfigurableMockLlmService`: configurable mock with request recording

use std::sync::{Arc, Mutex};

use crate::{CompletionRequest, CompletionResponse, LlmError, LlmService};

/// Mock LLM service for testing
#[derive(Debug, Clone)]
pub struct MockLlmService;

impl MockLlmService {
    /// Create a new mock LLM service
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockLlmService {
    fn default() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl LlmService for MockLlmService {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        tracing::info!("Mock LLM service processing completion request");

        let model = if request.model.is_empty() {
            "mock-model".to_string()
        } else {
            request.model
        };

        // Generate a simple response based on the last user message
        let last_message = request
            .messages
            .last()
            .map(|m| m.content.as_str())
            .unwrap_or("empty");

        let content = format!("Mock response to: {}", last_message);
        let input_tokens = request
            .messages
            .iter()
            .map(|m| m.content.len() as i32 / 4)
            .sum::<i32>();
        let output_tokens = content.len() as i32 / 4;

        Ok(CompletionResponse {
            content,
            model,
            input_tokens,
            output_tokens,
            stop_reason: "end_turn".to_string(),
        })
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }
}

#[cfg(any(test, feature = "test-support"))]
/// Configurable behavior for the mock LLM service
#[derive(Debug, Clone)]
pub enum MockBehavior {
    /// Always return this fixed content
    FixedResponse(String),
    /// Rotate through a sequence of responses
    ResponseSequence(Vec<String>),
    /// Simulate an LLM error
    Error(LlmErrorKind),
    /// Echo the last message (default behavior)
    Echo,
}

#[cfg(any(test, feature = "test-support"))]
/// Error kinds that can be simulated by the configurable mock
#[derive(Debug, Clone)]
pub enum LlmErrorKind {
    /// Simulate rate limiting
    RateLimit,
    /// Simulate a request error
    Request(String),
    /// Simulate a response error
    Response(String),
}

#[cfg(any(test, feature = "test-support"))]
/// A recorded LLM request for test assertions
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub model: String,
    pub system_prompt: Option<String>,
    pub messages: Vec<crate::LlmMessage>,
    pub max_tokens: Option<u32>,
}

#[cfg(any(test, feature = "test-support"))]
/// Configurable mock LLM service with request recording
///
/// Thread-safe via `Arc<Mutex<>>`, following the pattern of `MockEmailService`.
#[derive(Debug, Clone)]
pub struct ConfigurableMockLlmService {
    behavior: Arc<Mutex<MockBehavior>>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    call_count: Arc<Mutex<usize>>,
}

#[cfg(any(test, feature = "test-support"))]
impl ConfigurableMockLlmService {
    /// Create with default echo behavior
    pub fn new() -> Self {
        Self {
            behavior: Arc::new(Mutex::new(MockBehavior::Echo)),
            requests: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Create with a specific behavior
    pub fn with_behavior(behavior: MockBehavior) -> Self {
        Self {
            behavior: Arc::new(Mutex::new(behavior)),
            requests: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Set the behavior (can be changed between calls)
    pub fn set_behavior(&self, behavior: MockBehavior) {
        let mut b = self.behavior.lock().unwrap();
        *b = behavior;
    }

    /// Get all recorded requests
    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Get the total call count
    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }

    /// Clear recorded requests and reset call count
    pub fn reset(&self) {
        self.requests.lock().unwrap().clear();
        *self.call_count.lock().unwrap() = 0;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Default for ConfigurableMockLlmService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait::async_trait]
impl LlmService for ConfigurableMockLlmService {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        // Record the request
        {
            let recorded = RecordedRequest {
                model: request.model.clone(),
                system_prompt: request.system_prompt.clone(),
                messages: request.messages.clone(),
                max_tokens: request.max_tokens,
            };
            self.requests.lock().unwrap().push(recorded);
        }

        // Increment call count
        let current_count = {
            let mut count = self.call_count.lock().unwrap();
            *count += 1;
            *count
        };

        // Determine response based on behavior
        let behavior = self.behavior.lock().unwrap().clone();
        match behavior {
            MockBehavior::FixedResponse(content) => Ok(CompletionResponse {
                content: content.clone(),
                model: if request.model.is_empty() {
                    "mock-model".to_string()
                } else {
                    request.model
                },
                input_tokens: 10,
                output_tokens: content.len() as i32 / 4,
                stop_reason: "end_turn".to_string(),
            }),
            MockBehavior::ResponseSequence(responses) => {
                let idx = (current_count - 1) % responses.len();
                let content = responses[idx].clone();
                Ok(CompletionResponse {
                    content: content.clone(),
                    model: if request.model.is_empty() {
                        "mock-model".to_string()
                    } else {
                        request.model
                    },
                    input_tokens: 10,
                    output_tokens: content.len() as i32 / 4,
                    stop_reason: "end_turn".to_string(),
                })
            }
            MockBehavior::Error(kind) => match kind {
                LlmErrorKind::RateLimit => Err(LlmError::RateLimit),
                LlmErrorKind::Request(msg) => Err(LlmError::Request(msg)),
                LlmErrorKind::Response(msg) => Err(LlmError::Response(msg)),
            },
            MockBehavior::Echo => {
                let last_message = request
                    .messages
                    .last()
                    .map(|m| m.content.as_str())
                    .unwrap_or("empty");

                let content = format!("Mock response to: {}", last_message);
                let input_tokens = request
                    .messages
                    .iter()
                    .map(|m| m.content.len() as i32 / 4)
                    .sum::<i32>();
                let output_tokens = content.len() as i32 / 4;

                Ok(CompletionResponse {
                    content,
                    model: if request.model.is_empty() {
                        "mock-model".to_string()
                    } else {
                        request.model
                    },
                    input_tokens,
                    output_tokens,
                    stop_reason: "end_turn".to_string(),
                })
            }
        }
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmMessage, LlmRole};

    #[tokio::test]
    async fn test_mock_llm_service() {
        let service = MockLlmService::new();

        let request = CompletionRequest {
            model: String::new(),
            system_prompt: None,
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "Hello, world!".to_string(),
            }],
            max_tokens: None,
        };

        let response = service.complete(request).await.unwrap();

        assert!(response.content.contains("Hello, world!"));
        assert_eq!(response.model, "mock-model");
        assert_eq!(response.stop_reason, "end_turn");
        assert!(response.input_tokens > 0);
        assert!(response.output_tokens > 0);
    }

    #[tokio::test]
    async fn test_mock_uses_provided_model() {
        let service = MockLlmService::new();

        let request = CompletionRequest {
            model: "custom-model".to_string(),
            system_prompt: None,
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "Test".to_string(),
            }],
            max_tokens: Some(100),
        };

        let response = service.complete(request).await.unwrap();
        assert_eq!(response.model, "custom-model");
    }

    #[test]
    fn test_mock_default_model() {
        let service = MockLlmService::new();
        assert_eq!(service.default_model(), "mock-model");
    }

    // --- ConfigurableMockLlmService tests (LLM-U1 through LLM-U8) ---

    fn make_request(content: &str) -> CompletionRequest {
        CompletionRequest {
            model: String::new(),
            system_prompt: None,
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: content.to_string(),
            }],
            max_tokens: None,
        }
    }

    #[tokio::test]
    async fn test_configurable_mock_fixed_response() {
        // LLM-U1: Returns exact content
        let service = ConfigurableMockLlmService::with_behavior(MockBehavior::FixedResponse(
            "Fixed answer".to_string(),
        ));

        let response = service.complete(make_request("anything")).await.unwrap();
        assert_eq!(response.content, "Fixed answer");
        assert_eq!(response.stop_reason, "end_turn");
    }

    #[tokio::test]
    async fn test_configurable_mock_response_sequence() {
        // LLM-U2: Rotates through responses
        let service =
            ConfigurableMockLlmService::with_behavior(MockBehavior::ResponseSequence(vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string(),
            ]));

        let r1 = service.complete(make_request("a")).await.unwrap();
        let r2 = service.complete(make_request("b")).await.unwrap();
        let r3 = service.complete(make_request("c")).await.unwrap();
        // Wraps around
        let r4 = service.complete(make_request("d")).await.unwrap();

        assert_eq!(r1.content, "first");
        assert_eq!(r2.content, "second");
        assert_eq!(r3.content, "third");
        assert_eq!(r4.content, "first");
    }

    #[tokio::test]
    async fn test_configurable_mock_error_rate_limit() {
        // LLM-U3: Returns RateLimit error
        let service =
            ConfigurableMockLlmService::with_behavior(MockBehavior::Error(LlmErrorKind::RateLimit));

        let result = service.complete(make_request("hello")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::RateLimit));
    }

    #[tokio::test]
    async fn test_configurable_mock_error_request() {
        // LLM-U4: Returns Request error
        let service = ConfigurableMockLlmService::with_behavior(MockBehavior::Error(
            LlmErrorKind::Request("connection refused".to_string()),
        ));

        let result = service.complete(make_request("hello")).await;
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Request(msg) if msg == "connection refused"));
    }

    #[tokio::test]
    async fn test_configurable_mock_records_requests() {
        // LLM-U5: Captures request history
        let service = ConfigurableMockLlmService::new();

        let req = CompletionRequest {
            model: "claude-test".to_string(),
            system_prompt: Some("You are helpful".to_string()),
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "What is 2+2?".to_string(),
            }],
            max_tokens: Some(100),
        };

        service.complete(req).await.unwrap();

        let recorded = service.recorded_requests();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].model, "claude-test");
        assert_eq!(
            recorded[0].system_prompt.as_deref(),
            Some("You are helpful")
        );
        assert_eq!(recorded[0].messages.len(), 1);
        assert_eq!(recorded[0].messages[0].content, "What is 2+2?");
        assert_eq!(recorded[0].max_tokens, Some(100));
    }

    #[tokio::test]
    async fn test_configurable_mock_call_count() {
        // LLM-U6: Tracks invocation count
        let service = ConfigurableMockLlmService::new();

        assert_eq!(service.call_count(), 0);
        service.complete(make_request("a")).await.unwrap();
        assert_eq!(service.call_count(), 1);
        service.complete(make_request("b")).await.unwrap();
        assert_eq!(service.call_count(), 2);

        // Reset clears count
        service.reset();
        assert_eq!(service.call_count(), 0);
        assert!(service.recorded_requests().is_empty());
    }

    #[tokio::test]
    async fn test_configurable_mock_echo_default() {
        // LLM-U7: Echo behavior works (default)
        let service = ConfigurableMockLlmService::new();

        let response = service.complete(make_request("Hello world")).await.unwrap();
        assert!(response.content.contains("Hello world"));
        assert_eq!(response.model, "mock-model");
        assert_eq!(response.stop_reason, "end_turn");
    }

    #[tokio::test]
    async fn test_configurable_mock_thread_safety() {
        // LLM-U8: Concurrent access works
        let service = Arc::new(ConfigurableMockLlmService::with_behavior(
            MockBehavior::FixedResponse("concurrent".to_string()),
        ));

        let mut handles = vec![];
        for i in 0..10 {
            let svc = Arc::clone(&service);
            handles.push(tokio::spawn(async move {
                let msg = format!("message-{}", i);
                svc.complete(make_request(&msg)).await.unwrap()
            }));
        }

        for handle in handles {
            let resp = handle.await.unwrap();
            assert_eq!(resp.content, "concurrent");
        }

        assert_eq!(service.call_count(), 10);
        assert_eq!(service.recorded_requests().len(), 10);
    }
}
