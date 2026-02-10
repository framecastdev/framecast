//! Mock render admin endpoints for E2E testing

use axum::{extract::State, http::StatusCode, Json};
use framecast_common::{Error, Result};
use framecast_runpod::mock::MockOutcome;
use serde::{Deserialize, Serialize};

use crate::api::middleware::GenerationsState;

/// Request to configure mock render behavior
#[derive(Debug, Deserialize)]
pub struct ConfigureMockRequest {
    pub outcome: Option<String>,
    pub delay_ms: Option<u64>,
    pub progress_steps: Option<Vec<f64>>,
    pub error_payload: Option<serde_json::Value>,
    pub output_payload: Option<serde_json::Value>,
}

/// Response for mock history
#[derive(Debug, Serialize)]
pub struct MockHistoryEntry {
    pub generation_id: uuid::Uuid,
    pub spec_snapshot: serde_json::Value,
    pub options: serde_json::Value,
    pub callback_url: String,
}

/// Configure mock render behavior
pub async fn configure_mock(
    State(state): State<GenerationsState>,
    Json(req): Json<ConfigureMockRequest>,
) -> Result<StatusCode> {
    let behavior = state
        .mock_render_behavior
        .as_ref()
        .ok_or_else(|| Error::NotFound("Mock render service not enabled".to_string()))?;

    if let Some(outcome) = req.outcome {
        let mock_outcome = match outcome.as_str() {
            "complete" => MockOutcome::Complete,
            "fail" => MockOutcome::Fail,
            "timeout" => MockOutcome::Timeout,
            other => {
                return Err(Error::Validation(format!(
                    "Unknown outcome: '{}'. Valid values: complete, fail, timeout",
                    other
                )));
            }
        };
        behavior.set_outcome(mock_outcome);
    }

    if let Some(delay) = req.delay_ms {
        behavior.set_delay_ms(delay);
    }

    if let Some(steps) = req.progress_steps {
        behavior.set_progress_steps(steps);
    }

    if let Some(error) = req.error_payload {
        behavior.set_error_payload(error);
    }

    if let Some(output) = req.output_payload {
        behavior.set_output_payload(output);
    }

    Ok(StatusCode::OK)
}

/// Get mock render request history
pub async fn get_history(
    State(state): State<GenerationsState>,
) -> Result<Json<Vec<MockHistoryEntry>>> {
    let history = state
        .mock_render_history
        .as_ref()
        .ok_or_else(|| Error::NotFound("Mock render service not enabled".to_string()))?;

    let entries: Vec<MockHistoryEntry> = history
        .lock()
        .map_err(|e| Error::Internal(format!("Failed to lock mock history: {}", e)))?
        .iter()
        .map(|r| MockHistoryEntry {
            generation_id: r.generation_id,
            spec_snapshot: r.spec_snapshot.clone(),
            options: r.options.clone(),
            callback_url: r.callback_url.clone(),
        })
        .collect();

    Ok(Json(entries))
}

/// Reset mock render behavior and history
pub async fn reset_mock(State(state): State<GenerationsState>) -> Result<StatusCode> {
    let behavior = state
        .mock_render_behavior
        .as_ref()
        .ok_or_else(|| Error::NotFound("Mock render service not enabled".to_string()))?;
    let history = state
        .mock_render_history
        .as_ref()
        .ok_or_else(|| Error::NotFound("Mock render service not enabled".to_string()))?;

    behavior.reset();
    history
        .lock()
        .map_err(|e| Error::Internal(format!("Failed to lock mock history: {}", e)))?
        .clear();

    Ok(StatusCode::OK)
}
