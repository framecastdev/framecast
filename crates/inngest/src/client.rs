//! Inngest HTTP Client Implementation
//!
//! Real HTTP client that POSTs events to the Inngest event API
//! at `{base_url}/e/{event_key}`.

use crate::{InngestConfig, InngestError, InngestEvent, InngestService};

/// Real Inngest HTTP client for sending events to the Inngest event API.
pub struct InngestClient {
    http: reqwest::Client,
    event_url: String,
}

impl InngestClient {
    /// Create a new Inngest client from configuration.
    pub fn new(config: InngestConfig) -> Self {
        let event_url = format!(
            "{}/e/{}",
            config.base_url.trim_end_matches('/'),
            config.event_key
        );
        Self {
            http: reqwest::Client::new(),
            event_url,
        }
    }
}

#[async_trait::async_trait]
impl InngestService for InngestClient {
    async fn send_event(&self, event: InngestEvent) -> Result<(), InngestError> {
        self.send_events(vec![event]).await
    }

    async fn send_events(&self, events: Vec<InngestEvent>) -> Result<(), InngestError> {
        let response = self
            .http
            .post(&self.event_url)
            .json(&events)
            .send()
            .await
            .map_err(|e| InngestError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response body".to_string());
            return Err(InngestError::Response(format!(
                "Inngest API returned {}: {}",
                status, body
            )));
        }

        tracing::debug!(count = events.len(), "Inngest events sent successfully");
        Ok(())
    }
}
