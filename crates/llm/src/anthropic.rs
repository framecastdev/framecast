//! Anthropic Claude API Implementation
//!
//! Calls the Anthropic Messages API (https://api.anthropic.com/v1/messages)
//! using reqwest HTTP client.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{CompletionRequest, CompletionResponse, LlmConfig, LlmError, LlmService};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";

/// Anthropic Messages API request body
#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<MessageBody>,
}

#[derive(Debug, Serialize)]
struct MessageBody {
    role: String,
    content: String,
}

/// Anthropic Messages API response body
#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: i32,
    output_tokens: i32,
}

/// Anthropic API error response
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

/// Anthropic LLM service implementation
pub struct AnthropicService {
    client: Client,
    config: LlmConfig,
    base_url: String,
}

impl AnthropicService {
    /// Create a new Anthropic service
    pub fn new(config: LlmConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Self {
            client: Client::new(),
            config,
            base_url,
        }
    }
}

#[async_trait::async_trait]
impl LlmService for AnthropicService {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let model = if request.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.model
        };

        let max_tokens = request.max_tokens.unwrap_or(self.config.max_tokens);

        let messages: Vec<MessageBody> = request
            .messages
            .iter()
            .map(|m| MessageBody {
                role: match m.role {
                    crate::LlmRole::User => "user".to_string(),
                    crate::LlmRole::Assistant => "assistant".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let body = MessagesRequest {
            model: model.clone(),
            max_tokens,
            system: request.system_prompt,
            messages,
        };

        let url = format!("{}/v1/messages", self.base_url);

        tracing::debug!(model = %model, max_tokens = %max_tokens, "Sending Anthropic API request");

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Request(format!("HTTP request failed: {}", e)))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimit);
        }

        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());

            // Try to parse as API error
            if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&error_body) {
                return Err(LlmError::Response(format!(
                    "Anthropic API error ({}): {}",
                    error_response.error.error_type, error_response.error.message
                )));
            }

            return Err(LlmError::Response(format!(
                "Anthropic API returned {}: {}",
                status, error_body
            )));
        }

        let api_response: MessagesResponse = response
            .json()
            .await
            .map_err(|e| LlmError::Response(format!("Failed to parse response: {}", e)))?;

        // Extract text content from response blocks
        let content = api_response
            .content
            .iter()
            .filter_map(|block| {
                if block.content_type == "text" {
                    block.text.clone()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        Ok(CompletionResponse {
            content,
            model: api_response.model,
            input_tokens: api_response.usage.input_tokens,
            output_tokens: api_response.usage.output_tokens,
            stop_reason: api_response
                .stop_reason
                .unwrap_or_else(|| "end_turn".to_string()),
            artifacts: Vec::new(),
        })
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }
}
