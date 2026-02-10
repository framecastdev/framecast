//! Job callback handler for render service postbacks

use axum::{extract::State, Json};
use framecast_common::{Error, Result};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::middleware::JobsState;
use crate::domain::entities::{JobEventType, JobFailureType};
use crate::repository::transactions::update_artifact_status_by_job;

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
            let updated = state.repos.jobs.update(&job).await?;

            let next_seq = state.repos.job_events.next_sequence(job.id).await?;
            state
                .repos
                .job_events
                .create(
                    job.id,
                    next_seq,
                    JobEventType::Started,
                    serde_json::json!({}),
                )
                .await?;

            Ok(Json(updated.into()))
        }
        "progress" => {
            if let Some(percent) = payload.progress_percent {
                job.update_progress(percent)?;
            }
            let updated = state.repos.jobs.update(&job).await?;

            let next_seq = state.repos.job_events.next_sequence(job.id).await?;
            state
                .repos
                .job_events
                .create(
                    job.id,
                    next_seq,
                    JobEventType::Progress,
                    serde_json::json!({
                        "percent": payload.progress_percent
                    }),
                )
                .await?;

            Ok(Json(updated.into()))
        }
        "completed" => {
            let output = payload.output.unwrap_or_else(|| serde_json::json!({}));
            job.complete(output, payload.output_size_bytes)?;
            let updated = state.repos.jobs.update(&job).await?;

            let next_seq = state.repos.job_events.next_sequence(job.id).await?;
            state
                .repos
                .job_events
                .create(
                    job.id,
                    next_seq,
                    JobEventType::Completed,
                    serde_json::json!({}),
                )
                .await?;

            // CQRS: update artifact status to "ready"
            let mut tx = state.repos.begin().await?;
            update_artifact_status_by_job(&mut tx, job.id, "ready").await?;
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
            let updated = state.repos.jobs.update(&job).await?;

            let next_seq = state.repos.job_events.next_sequence(job.id).await?;
            state
                .repos
                .job_events
                .create(
                    job.id,
                    next_seq,
                    JobEventType::Failed,
                    serde_json::json!({}),
                )
                .await?;

            // CQRS: update artifact status to "failed"
            let mut tx = state.repos.begin().await?;
            update_artifact_status_by_job(&mut tx, job.id, "failed").await?;
            tx.commit().await?;

            Ok(Json(updated.into()))
        }
        unknown => Err(Error::Validation(format!(
            "Unknown callback event: '{}'",
            unknown
        ))),
    }
}
