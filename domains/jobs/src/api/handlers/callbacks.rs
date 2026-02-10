//! Job callback handler for render service postbacks

use axum::{extract::State, Json};
use framecast_common::{Error, Result};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::middleware::JobsState;
use crate::domain::entities::{JobEventType, JobFailureType};
use crate::repository::transactions::{
    create_job_event_tx, next_sequence_tx, update_artifact_status_by_job, update_job_tx,
};

use super::jobs::JobResponse;

/// Callback payload from render service
#[derive(Debug, Deserialize)]
pub struct JobCallbackPayload {
    pub job_id: Uuid,
    pub event: String,
    pub output: Option<serde_json::Value>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<serde_json::Value>,
    pub failure_type: Option<String>,
    pub progress_percent: Option<f64>,
}

/// Handle job callback from render service (internal, no auth)
pub async fn job_callback(
    State(state): State<JobsState>,
    Json(payload): Json<JobCallbackPayload>,
) -> Result<Json<JobResponse>> {
    let mut job = state
        .repos
        .jobs
        .find(payload.job_id)
        .await?
        .ok_or_else(|| Error::NotFound("Job not found".to_string()))?;

    match payload.event.as_str() {
        "started" => {
            job.start()?;

            let mut tx = state.repos.begin().await?;
            let updated = update_job_tx(&mut tx, &job).await?;
            let next_seq = next_sequence_tx(&mut tx, job.id).await?;
            create_job_event_tx(
                &mut tx,
                job.id,
                next_seq,
                JobEventType::Started,
                serde_json::json!({}),
            )
            .await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        "progress" => {
            if let Some(percent) = payload.progress_percent {
                job.update_progress(percent)?;
            }

            let mut tx = state.repos.begin().await?;
            let updated = update_job_tx(&mut tx, &job).await?;
            let next_seq = next_sequence_tx(&mut tx, job.id).await?;
            create_job_event_tx(
                &mut tx,
                job.id,
                next_seq,
                JobEventType::Progress,
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
            job.complete(output, payload.output_size_bytes)?;

            let mut tx = state.repos.begin().await?;
            let updated = update_job_tx(&mut tx, &job).await?;
            let next_seq = next_sequence_tx(&mut tx, job.id).await?;
            create_job_event_tx(
                &mut tx,
                job.id,
                next_seq,
                JobEventType::Completed,
                serde_json::json!({}),
            )
            .await?;
            update_artifact_status_by_job(&mut tx, job.id, "ready", payload.output_size_bytes)
                .await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        "failed" => {
            let error = payload
                .error
                .unwrap_or_else(|| serde_json::json!({"message": "Unknown error"}));
            let failure_type = match payload.failure_type.as_deref() {
                Some("validation") => JobFailureType::Validation,
                Some("timeout") => JobFailureType::Timeout,
                Some("canceled") => JobFailureType::Canceled,
                _ => JobFailureType::System,
            };
            job.fail(error, failure_type)?;

            let mut tx = state.repos.begin().await?;
            let updated = update_job_tx(&mut tx, &job).await?;
            let next_seq = next_sequence_tx(&mut tx, job.id).await?;
            create_job_event_tx(
                &mut tx,
                job.id,
                next_seq,
                JobEventType::Failed,
                serde_json::json!({}),
            )
            .await?;
            update_artifact_status_by_job(&mut tx, job.id, "failed", None).await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        unknown => Err(Error::Validation(format!(
            "Unknown callback event: '{}'",
            unknown
        ))),
    }
}
