//! Generation callback handler for render service postbacks

use axum::{extract::State, Json};
use framecast_common::{Error, Result};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::middleware::GenerationsState;
use crate::domain::entities::{GenerationEventType, GenerationFailureType};
use crate::repository::transactions::{
    create_generation_event_tx, next_sequence_tx, update_artifact_status_by_generation,
    update_generation_tx,
};

use super::generations::GenerationResponse;

/// Callback payload from render service
#[derive(Debug, Deserialize)]
pub struct GenerationCallbackPayload {
    pub generation_id: Uuid,
    pub event: String,
    pub output: Option<serde_json::Value>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<serde_json::Value>,
    pub failure_type: Option<String>,
    pub progress_percent: Option<f64>,
}

/// Handle generation callback from render service (internal, no auth)
pub async fn generation_callback(
    State(state): State<GenerationsState>,
    Json(payload): Json<GenerationCallbackPayload>,
) -> Result<Json<GenerationResponse>> {
    let mut generation = state
        .repos
        .generations
        .find(payload.generation_id)
        .await?
        .ok_or_else(|| Error::NotFound("Generation not found".to_string()))?;

    match payload.event.as_str() {
        "started" => {
            generation.start()?;

            let mut tx = state.repos.begin().await?;
            let updated = update_generation_tx(&mut tx, &generation).await?;
            let next_seq = next_sequence_tx(&mut tx, generation.id).await?;
            create_generation_event_tx(
                &mut tx,
                generation.id,
                next_seq,
                GenerationEventType::Started,
                serde_json::json!({}),
            )
            .await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        "progress" => {
            if let Some(percent) = payload.progress_percent {
                generation.update_progress(percent)?;
            }

            let mut tx = state.repos.begin().await?;
            let updated = update_generation_tx(&mut tx, &generation).await?;
            let next_seq = next_sequence_tx(&mut tx, generation.id).await?;
            create_generation_event_tx(
                &mut tx,
                generation.id,
                next_seq,
                GenerationEventType::Progress,
                serde_json::json!({
                    "percent": payload.progress_percent
                }),
            )
            .await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        "completed" => {
            let output = payload.output.unwrap_or_else(|| serde_json::json!({}));
            generation.complete(output, payload.output_size_bytes)?;

            let mut tx = state.repos.begin().await?;
            let updated = update_generation_tx(&mut tx, &generation).await?;
            let next_seq = next_sequence_tx(&mut tx, generation.id).await?;
            create_generation_event_tx(
                &mut tx,
                generation.id,
                next_seq,
                GenerationEventType::Completed,
                serde_json::json!({}),
            )
            .await?;
            update_artifact_status_by_generation(
                &mut tx,
                generation.id,
                "ready",
                payload.output_size_bytes,
            )
            .await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        "failed" => {
            let error = payload
                .error
                .unwrap_or_else(|| serde_json::json!({"message": "Unknown error"}));
            let failure_type = match payload.failure_type.as_deref() {
                Some("validation") => GenerationFailureType::Validation,
                Some("timeout") => GenerationFailureType::Timeout,
                Some("canceled") => GenerationFailureType::Canceled,
                _ => GenerationFailureType::System,
            };
            generation.fail(error, failure_type)?;

            let mut tx = state.repos.begin().await?;
            let updated = update_generation_tx(&mut tx, &generation).await?;
            let next_seq = next_sequence_tx(&mut tx, generation.id).await?;
            create_generation_event_tx(
                &mut tx,
                generation.id,
                next_seq,
                GenerationEventType::Failed,
                serde_json::json!({}),
            )
            .await?;
            update_artifact_status_by_generation(&mut tx, generation.id, "failed", None).await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        unknown => Err(Error::Validation(format!(
            "Unknown callback event: '{}'",
            unknown
        ))),
    }
}
