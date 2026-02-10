//! Mock Render Service Implementation
//!
//! Programmable mock for testing render workflows:
//! - `MockRenderService`: configurable mock with request recording
//! - `MockRenderBehavior`: controls outcome, delay, progress steps
//! - `MockOutcome`: Complete, Fail, or Timeout

use crate::{RenderError, RenderRequest, RenderResult, RenderService};
use std::sync::{Arc, RwLock};

/// What outcome the mock should produce
#[derive(Debug, Clone, Default, PartialEq)]
pub enum MockOutcome {
    /// Post success callback with output
    #[default]
    Complete,
    /// Post failure callback with error
    Fail,
    /// Never post callback (simulates timeout)
    Timeout,
}

/// Programmable behavior for the mock render service
#[derive(Debug, Clone)]
pub struct MockRenderBehavior {
    pub outcome: Arc<RwLock<MockOutcome>>,
    pub delay_ms: Arc<RwLock<u64>>,
    pub progress_steps: Arc<RwLock<Vec<f64>>>,
    pub error_payload: Arc<RwLock<Option<serde_json::Value>>>,
    pub output_payload: Arc<RwLock<Option<serde_json::Value>>>,
}

impl Default for MockRenderBehavior {
    fn default() -> Self {
        Self {
            outcome: Arc::new(RwLock::new(MockOutcome::Complete)),
            delay_ms: Arc::new(RwLock::new(50)),
            progress_steps: Arc::new(RwLock::new(Vec::new())),
            error_payload: Arc::new(RwLock::new(None)),
            output_payload: Arc::new(RwLock::new(None)),
        }
    }
}

impl MockRenderBehavior {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the mock outcome
    pub fn set_outcome(&self, outcome: MockOutcome) {
        *self.outcome.write().unwrap() = outcome;
    }

    /// Configure delay before postback
    pub fn set_delay_ms(&self, delay: u64) {
        *self.delay_ms.write().unwrap() = delay;
    }

    /// Configure progress steps to send before final callback
    pub fn set_progress_steps(&self, steps: Vec<f64>) {
        *self.progress_steps.write().unwrap() = steps;
    }

    /// Configure error payload for failure outcome
    pub fn set_error_payload(&self, payload: serde_json::Value) {
        *self.error_payload.write().unwrap() = Some(payload);
    }

    /// Configure output payload for success outcome
    pub fn set_output_payload(&self, payload: serde_json::Value) {
        *self.output_payload.write().unwrap() = Some(payload);
    }

    /// Reset to default behavior
    pub fn reset(&self) {
        *self.outcome.write().unwrap() = MockOutcome::Complete;
        *self.delay_ms.write().unwrap() = 50;
        *self.progress_steps.write().unwrap() = Vec::new();
        *self.error_payload.write().unwrap() = None;
        *self.output_payload.write().unwrap() = None;
    }

    /// Read current outcome
    pub fn get_outcome(&self) -> MockOutcome {
        self.outcome.read().unwrap().clone()
    }

    /// Read current delay
    pub fn get_delay_ms(&self) -> u64 {
        *self.delay_ms.read().unwrap()
    }
}

/// A recorded render request for test assertions
#[derive(Debug, Clone)]
pub struct RecordedRenderRequest {
    pub generation_id: uuid::Uuid,
    pub spec_snapshot: serde_json::Value,
    pub options: serde_json::Value,
    pub callback_url: String,
}

/// Mock render service with programmable behavior
#[derive(Debug, Clone)]
pub struct MockRenderService {
    behavior: Arc<MockRenderBehavior>,
    history: Arc<std::sync::Mutex<Vec<RecordedRenderRequest>>>,
    callback_base_url: String,
}

impl MockRenderService {
    pub fn new(callback_base_url: String) -> Self {
        Self {
            behavior: Arc::new(MockRenderBehavior::new()),
            history: Arc::new(std::sync::Mutex::new(Vec::new())),
            callback_base_url,
        }
    }

    pub fn with_behavior(behavior: Arc<MockRenderBehavior>, callback_base_url: String) -> Self {
        Self {
            behavior,
            history: Arc::new(std::sync::Mutex::new(Vec::new())),
            callback_base_url,
        }
    }

    /// Get the shared behavior for external configuration (e.g. admin endpoints)
    pub fn behavior(&self) -> &Arc<MockRenderBehavior> {
        &self.behavior
    }

    /// Get recorded render requests
    pub fn recorded_requests(&self) -> Vec<RecordedRenderRequest> {
        self.history.lock().unwrap().clone()
    }

    /// Get request history (shared ref for admin endpoints)
    pub fn history(&self) -> &Arc<std::sync::Mutex<Vec<RecordedRenderRequest>>> {
        &self.history
    }

    /// Clear history
    pub fn reset_history(&self) {
        self.history.lock().unwrap().clear();
    }

    /// Get the callback base URL
    pub fn callback_base_url(&self) -> &str {
        &self.callback_base_url
    }
}

#[async_trait::async_trait]
impl RenderService for MockRenderService {
    async fn submit_render(&self, request: RenderRequest) -> Result<(), RenderError> {
        tracing::info!(generation_id = %request.generation_id, "Mock render: received render request");

        // Record the request
        {
            let recorded = RecordedRenderRequest {
                generation_id: request.generation_id,
                spec_snapshot: request.spec_snapshot.clone(),
                options: request.options.clone(),
                callback_url: request.callback_url.clone(),
            };
            self.history.lock().unwrap().push(recorded);
        }

        // Read behavior settings
        let outcome = self.behavior.get_outcome();
        let delay_ms = self.behavior.get_delay_ms();
        let progress_steps = self.behavior.progress_steps.read().unwrap().clone();
        let error_payload = self.behavior.error_payload.read().unwrap().clone();
        let output_payload = self.behavior.output_payload.read().unwrap().clone();

        // If timeout, just return without posting anything
        if outcome == MockOutcome::Timeout {
            tracing::info!(generation_id = %request.generation_id, "Mock render: simulating timeout (no callback)");
            return Ok(());
        }

        let callback_url = request.callback_url.clone();
        let generation_id = request.generation_id;

        // Spawn async task to simulate postback
        tokio::spawn(async move {
            let client = reqwest::Client::new();

            // Wait initial delay
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

            // Send "started" callback
            let started_payload = serde_json::json!({
                "generation_id": generation_id,
                "event": "started"
            });
            let _ = client
                .post(&callback_url)
                .json(&started_payload)
                .send()
                .await;

            // Send progress steps
            for step in &progress_steps {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                let progress_payload = serde_json::json!({
                    "generation_id": generation_id,
                    "event": "progress",
                    "progress_percent": step
                });
                let _ = client
                    .post(&callback_url)
                    .json(&progress_payload)
                    .send()
                    .await;
            }

            // Small delay before final callback
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

            // Send final callback based on outcome
            match outcome {
                MockOutcome::Complete => {
                    let result = RenderResult {
                        generation_id,
                        status: "completed".to_string(),
                        output: Some(output_payload.unwrap_or_else(|| {
                            serde_json::json!({"url": format!("https://mock-storage.example.com/renders/{}.mp4", generation_id)})
                        })),
                        output_size_bytes: Some(1024),
                        error: None,
                    };
                    let completed_payload = serde_json::json!({
                        "generation_id": generation_id,
                        "event": "completed",
                        "output": result.output,
                        "output_size_bytes": result.output_size_bytes
                    });
                    let _ = client
                        .post(&callback_url)
                        .json(&completed_payload)
                        .send()
                        .await;
                }
                MockOutcome::Fail => {
                    let error = error_payload.unwrap_or_else(|| {
                        serde_json::json!({"message": "Mock render failure", "code": "MOCK_ERROR"})
                    });
                    let failed_payload = serde_json::json!({
                        "generation_id": generation_id,
                        "event": "failed",
                        "error": error,
                        "failure_type": "system"
                    });
                    let _ = client
                        .post(&callback_url)
                        .json(&failed_payload)
                        .send()
                        .await;
                }
                MockOutcome::Timeout => unreachable!(),
            }

            tracing::info!(generation_id = %generation_id, "Mock render: postback sequence completed");
        });

        Ok(())
    }
}
