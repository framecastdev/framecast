//! Mock LLM Service Implementation
//!
//! Minimal mock used by `LlmServiceFactory` when provider is `"mock"`.
//! Returns deterministic responses for testing.

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
}
