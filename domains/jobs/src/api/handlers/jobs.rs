//! Job management API handlers

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, Sse},
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AnyAuth;
use framecast_common::{Error, Result, Urn};
use framecast_inngest::InngestEvent;
use framecast_runpod::RenderRequest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::JobsState;
use crate::domain::entities::{Job, JobEventType, JobFailureType, JobStatus};
use crate::repository::transactions::{
    count_active_for_owner_tx, create_job_event_tx, create_job_tx,
};

/// Job response DTO
#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: Uuid,
    pub owner: String,
    pub triggered_by: Uuid,
    pub project_id: Option<Uuid>,
    pub status: JobStatus,
    pub spec_snapshot: serde_json::Value,
    pub options: serde_json::Value,
    pub progress: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<serde_json::Value>,
    pub credits_charged: i32,
    pub failure_type: Option<JobFailureType>,
    pub credits_refunded: i32,
    pub idempotency_key: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Job> for JobResponse {
    fn from(j: Job) -> Self {
        Self {
            id: j.id,
            owner: j.owner,
            triggered_by: j.triggered_by,
            project_id: j.project_id,
            status: j.status,
            spec_snapshot: j.spec_snapshot.0,
            options: j.options.0,
            progress: j.progress.0,
            output: j.output.map(|o| o.0),
            output_size_bytes: j.output_size_bytes,
            error: j.error.map(|e| e.0),
            credits_charged: j.credits_charged,
            failure_type: j.failure_type,
            credits_refunded: j.credits_refunded,
            idempotency_key: j.idempotency_key,
            started_at: j.started_at,
            completed_at: j.completed_at,
            created_at: j.created_at,
            updated_at: j.updated_at,
        }
    }
}

/// Query parameters for listing jobs
#[derive(Debug, Deserialize)]
pub struct ListJobsParams {
    pub status: Option<JobStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Request for creating an ephemeral job
#[derive(Debug, Deserialize)]
pub struct CreateEphemeralJobRequest {
    pub spec: serde_json::Value,
    pub owner: Option<String>,
    pub options: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
}

/// Request for cloning a job
#[derive(Debug, Deserialize)]
pub struct CloneJobRequest {
    pub owner: Option<String>,
}

/// Lightweight artifact read model for CQRS cross-domain query
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ArtifactReadModel {
    pub id: Uuid,
    pub owner: String,
    pub created_by: Uuid,
    pub project_id: Option<Uuid>,
    pub kind: String,
    pub status: String,
    pub source: String,
    pub spec: Option<serde_json::Value>,
    pub source_job_id: Option<Uuid>,
    pub filename: Option<String>,
    pub s3_key: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Lightweight artifact response for render endpoint
#[derive(Debug, Serialize)]
pub struct RenderArtifactResponse {
    pub id: Uuid,
    pub owner: String,
    pub kind: String,
    pub status: String,
    pub source: String,
    pub source_job_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Combined response for render artifact endpoint
#[derive(Debug, Serialize)]
pub struct RenderResponse {
    pub job: JobResponse,
    pub artifact: RenderArtifactResponse,
}

/// List jobs for the authenticated user (personal + team-owned)
pub async fn list_jobs(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Query(params): Query<ListJobsParams>,
) -> Result<Json<Vec<JobResponse>>> {
    // Collect all owner URNs the user can access: personal + each team
    let mut owner_urns = vec![Urn::user(ctx.user.id).to_string()];
    for membership in &ctx.memberships {
        owner_urns.push(Urn::team(membership.team_id).to_string());
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    let jobs = state
        .repos
        .jobs
        .list_by_owners(&owner_urns, params.status.as_ref(), limit, offset)
        .await?;

    let responses: Vec<JobResponse> = jobs.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Get a single job by ID
pub async fn get_job(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<JobResponse>> {
    let job = state
        .repos
        .jobs
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Job not found".to_string()))?;

    // Authorization: check ownership (personal or team membership)
    let owner_urn = job
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on job".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Job not found".to_string()));
    }

    Ok(Json(job.into()))
}

/// Create an ephemeral job (not tied to a project)
pub async fn create_ephemeral_job(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Json(req): Json<CreateEphemeralJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>)> {
    // Resolve owner
    let owner = match req.owner {
        Some(ref urn_str) => {
            let urn = urn_str
                .parse::<Urn>()
                .map_err(|_| Error::Validation("Invalid owner URN".to_string()))?;
            if !ctx.can_access_urn(&urn) {
                return Err(Error::Authorization(
                    "You do not have access to the specified owner".to_string(),
                ));
            }
            // Starter users cannot use team URNs
            if ctx.is_starter() && urn.is_team() {
                return Err(Error::Authorization(
                    "Starter users cannot create jobs for teams".to_string(),
                ));
            }
            urn
        }
        None => Urn::user(ctx.user.id),
    };

    // Check idempotency
    if let Some(ref key) = req.idempotency_key {
        if let Some(existing_job) = state
            .repos
            .jobs
            .find_by_idempotency_key(ctx.user.id, key)
            .await?
        {
            return Ok((StatusCode::OK, Json(existing_job.into())));
        }
    }

    // Create job
    let job = Job::new(
        owner,
        ctx.user.id,
        None,
        req.spec.clone(),
        req.options.clone(),
        0, // credits deferred
        req.idempotency_key,
    )?;

    // Transaction: check concurrency (FOR UPDATE lock) + create job + initial event
    let mut tx = state.repos.begin().await?;
    let active_count = count_active_for_owner_tx(&mut tx, &job.owner).await?;
    let max_concurrent = if ctx.is_starter() { 1 } else { 5 };
    if active_count >= max_concurrent {
        return Err(Error::Conflict(format!(
            "Concurrency limit exceeded: {} active jobs (max {})",
            active_count, max_concurrent
        )));
    }
    let created_job = create_job_tx(&mut tx, &job).await?;
    create_job_event_tx(
        &mut tx,
        created_job.id,
        1,
        JobEventType::Queued,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;

    // Send Inngest event
    let inngest_event = InngestEvent {
        name: "framecast/job.queued".to_string(),
        data: serde_json::json!({
            "job_id": created_job.id,
            "owner": created_job.owner,
            "spec_snapshot": req.spec,
            "options": req.options.unwrap_or_else(|| serde_json::json!({}))
        }),
        user: None,
        id: None,
        ts: None,
    };
    if let Err(e) = state.inngest.send_event(inngest_event).await {
        tracing::error!(error = %e, job_id = %created_job.id, "Failed to send Inngest event");
    }

    Ok((StatusCode::CREATED, Json(created_job.into())))
}

/// Cancel a job
pub async fn cancel_job(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<JobResponse>> {
    let mut job = state
        .repos
        .jobs
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Job not found".to_string()))?;

    // Authorization: triggered_by matches OR team owner/admin role
    let owner_urn = job
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on job".to_string()))?;

    let authorized = if job.triggered_by == ctx.user.id {
        true
    } else if let Ok(framecast_common::UrnComponents::Team { team_id }) = owner_urn.parse() {
        ctx.get_team_role(team_id)
            .map(|role| role.can_admin())
            .unwrap_or(false)
    } else {
        false
    };

    if !authorized {
        return Err(Error::NotFound("Job not found".to_string()));
    }

    // Check if terminal
    if job.is_terminal() {
        return Err(Error::Conflict(format!(
            "Job is already in terminal state '{:?}'",
            job.status
        )));
    }

    // Cancel job (state machine transition)
    job.cancel()?;

    // Update repo
    let updated_job = state.repos.jobs.update(&job).await?;

    // Create job event
    let next_seq = state.repos.job_events.next_sequence(id).await?;
    state
        .repos
        .job_events
        .create(id, next_seq, JobEventType::Canceled, serde_json::json!({}))
        .await?;

    Ok(Json(updated_job.into()))
}

/// Delete a job (only ephemeral + terminal jobs)
pub async fn delete_job(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let job = state
        .repos
        .jobs
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Job not found".to_string()))?;

    // Authorization: check ownership
    let owner_urn = job
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on job".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Job not found".to_string()));
    }

    // Must be ephemeral (no project) and terminal
    if !job.is_ephemeral() {
        return Err(Error::Validation(
            "Only ephemeral jobs (without project) can be deleted".to_string(),
        ));
    }
    if !job.is_terminal() {
        return Err(Error::Validation(
            "Only terminal jobs can be deleted".to_string(),
        ));
    }

    state.repos.jobs.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clone a job (creates a new job from an existing terminal job)
pub async fn clone_job(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Path(id): Path<Uuid>,
    body: Option<Json<CloneJobRequest>>,
) -> Result<(StatusCode, Json<JobResponse>)> {
    let original = state
        .repos
        .jobs
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Job not found".to_string()))?;

    // Authorization: check ownership of original
    let owner_urn = original
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on job".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Job not found".to_string()));
    }

    // Original must be terminal
    if !original.is_terminal() {
        return Err(Error::Validation(
            "Cannot clone an active job; job must be in a terminal state".to_string(),
        ));
    }

    // Resolve owner for new job
    let new_owner = match body.as_ref().and_then(|b| b.owner.as_ref()) {
        Some(urn_str) => {
            let urn = urn_str
                .parse::<Urn>()
                .map_err(|_| Error::Validation("Invalid owner URN".to_string()))?;
            if !ctx.can_access_urn(&urn) {
                return Err(Error::Authorization(
                    "You do not have access to the specified owner".to_string(),
                ));
            }
            urn
        }
        None => owner_urn,
    };

    // Create new job from original's spec and options
    let new_job = Job::new(
        new_owner,
        ctx.user.id,
        None,
        original.spec_snapshot.0,
        Some(original.options.0.clone()),
        0, // credits deferred
        None,
    )?;

    // Transaction: check concurrency (FOR UPDATE lock) + create job + initial event
    let mut tx = state.repos.begin().await?;
    let active_count = count_active_for_owner_tx(&mut tx, &new_job.owner).await?;
    let max_concurrent = if ctx.is_starter() { 1 } else { 5 };
    if active_count >= max_concurrent {
        return Err(Error::Conflict(format!(
            "Concurrency limit exceeded: {} active jobs (max {})",
            active_count, max_concurrent
        )));
    }
    let created_job = create_job_tx(&mut tx, &new_job).await?;
    create_job_event_tx(
        &mut tx,
        created_job.id,
        1,
        JobEventType::Queued,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;

    // Send Inngest event
    let inngest_event = InngestEvent {
        name: "framecast/job.queued".to_string(),
        data: serde_json::json!({
            "job_id": created_job.id,
            "owner": created_job.owner,
            "spec_snapshot": created_job.spec_snapshot.0,
            "options": created_job.options.0
        }),
        user: None,
        id: None,
        ts: None,
    };
    if let Err(e) = state.inngest.send_event(inngest_event).await {
        tracing::error!(error = %e, job_id = %created_job.id, "Failed to send Inngest event");
    }

    Ok((StatusCode::CREATED, Json(created_job.into())))
}

/// Get job events (SSE stream)
pub async fn get_job_events(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<
    Sse<impl futures_core::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    let job = state
        .repos
        .jobs
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Job not found".to_string()))?;

    // Authorization: check ownership
    let owner_urn = job
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on job".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Job not found".to_string()));
    }

    // Parse Last-Event-ID header to get last seen sequence
    let after_sequence = headers
        .get("Last-Event-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            // Format: {job_id}:{sequence}
            v.rsplit(':').next().and_then(|s| s.parse::<i64>().ok())
        });

    let job_id = id;
    let repos = state.repos.clone();

    let stream = async_stream::stream! {
        let mut last_seq = after_sequence.unwrap_or(0);
        let mut iterations: u32 = 0;
        // 15-minute maximum duration at 1s intervals prevents resource leak from stuck jobs
        const MAX_ITERATIONS: u32 = 900;

        loop {
            // Query for new events
            let events = match repos.job_events.list_by_job(job_id, Some(last_seq)).await {
                Ok(events) => events,
                Err(_) => break,
            };

            for event in &events {
                let event_type_str = serde_json::to_string(&event.event_type)
                    .unwrap_or_else(|_| "unknown".to_string())
                    .replace('"', "");
                let data = serde_json::to_string(&event.payload.0)
                    .unwrap_or_else(|_| "{}".to_string());

                let sse_event = Event::default()
                    .id(format!("{}:{}", job_id, event.sequence))
                    .event(event_type_str)
                    .data(data);

                yield Ok(sse_event);
                last_seq = event.sequence;
            }

            // Check if job is terminal — if so, close the stream
            let current_job = match repos.jobs.find(job_id).await {
                Ok(Some(j)) => j,
                _ => break,
            };
            if current_job.is_terminal() {
                break;
            }

            iterations += 1;
            if iterations >= MAX_ITERATIONS {
                break;
            }

            // Poll every 1 second for new events
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    };

    Ok(Sse::new(stream))
}

/// Render an artifact (creates a job + pending output artifact)
pub async fn render_artifact(
    AnyAuth(ctx): AnyAuth,
    State(state): State<JobsState>,
    Path(artifact_id): Path<Uuid>,
) -> Result<(StatusCode, Json<RenderResponse>)> {
    // CQRS read: query artifact directly from artifacts table
    let artifact: ArtifactReadModel = sqlx::query_as::<_, ArtifactReadModel>(
        r#"
        SELECT id, owner, created_by, project_id, kind::text as kind, status::text as status,
               source::text as source, spec, source_job_id, filename, s3_key, content_type,
               size_bytes, created_at, updated_at
        FROM artifacts WHERE id = $1
        "#,
    )
    .bind(artifact_id)
    .fetch_optional(state.repos.pool())
    .await?
    .ok_or_else(|| Error::NotFound("Artifact not found".to_string()))?;

    // Authorization: check ownership
    let owner_urn = artifact
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on artifact".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Artifact not found".to_string()));
    }

    // Validate renderable kind and determine output kind
    let (output_kind, output_filename, output_s3_key, output_content_type) =
        match artifact.kind.as_str() {
            "character" => (
                "image",
                format!("render-{}.png", artifact_id),
                format!("renders/{}/{}.png", ctx.user.id, Uuid::new_v4()),
                "image/png",
            ),
            "storyboard" => (
                "video",
                format!("render-{}.mp4", artifact_id),
                format!("renders/{}/{}.mp4", ctx.user.id, Uuid::new_v4()),
                "video/mp4",
            ),
            other => {
                return Err(Error::Validation(format!(
                    "Artifact kind '{}' is not renderable",
                    other
                )));
            }
        };

    // Create Job from the source artifact's spec
    let spec = artifact
        .spec
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let job = Job::new(
        owner_urn.clone(),
        ctx.user.id,
        None,
        spec,
        None,
        0, // credits deferred
        None,
    )?;

    // Transaction: check concurrency (FOR UPDATE lock) + create job + artifact + job event
    let mut tx = state.repos.begin().await?;
    let active_count = count_active_for_owner_tx(&mut tx, &job.owner).await?;
    let max_concurrent = if ctx.is_starter() { 1 } else { 5 };
    if active_count >= max_concurrent {
        return Err(Error::Conflict(format!(
            "Concurrency limit exceeded: {} active jobs (max {})",
            active_count, max_concurrent
        )));
    }

    let created_job = create_job_tx(&mut tx, &job).await?;

    // CQRS write: INSERT directly into artifacts table
    let output_artifact_id = Uuid::new_v4();
    let now = Utc::now();
    let output_artifact: ArtifactReadModel = sqlx::query_as::<_, ArtifactReadModel>(
        r#"
        INSERT INTO artifacts (
            id, owner, created_by, project_id,
            kind, status, source,
            filename, s3_key, content_type, size_bytes,
            spec, conversation_id, source_job_id,
            metadata, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5::artifact_kind, 'pending'::asset_status, 'job'::artifact_source,
                $6, $7, $8, $9, $10, NULL, $11, '{}'::jsonb, $12, $13)
        RETURNING id, owner, created_by, project_id, kind::text as kind, status::text as status,
                  source::text as source, spec, source_job_id, filename, s3_key, content_type,
                  size_bytes, created_at, updated_at
        "#,
    )
    .bind(output_artifact_id)
    .bind(&artifact.owner)
    .bind(ctx.user.id)
    .bind(artifact.project_id)
    .bind(output_kind)
    .bind(&output_filename)
    .bind(&output_s3_key)
    .bind(output_content_type)
    .bind(1_i64) // placeholder; updated on job completion via callback
    .bind(&artifact.spec)
    .bind(created_job.id)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    create_job_event_tx(
        &mut tx,
        created_job.id,
        1,
        JobEventType::Queued,
        serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    // Submit to render service
    let render_request = RenderRequest {
        job_id: created_job.id,
        spec_snapshot: created_job.spec_snapshot.0.clone(),
        options: created_job.options.0.clone(),
        callback_url: format!("{}/internal/jobs/callback", state.callback_base_url),
    };
    if let Err(e) = state.render.submit_render(render_request).await {
        tracing::error!(error = %e, job_id = %created_job.id, "Failed to submit render request");
    }

    // Send Inngest event
    let inngest_event = InngestEvent {
        name: "framecast/job.queued".to_string(),
        data: serde_json::json!({
            "job_id": created_job.id,
            "owner": created_job.owner,
            "spec_snapshot": created_job.spec_snapshot.0,
            "options": created_job.options.0
        }),
        user: None,
        id: None,
        ts: None,
    };
    if let Err(e) = state.inngest.send_event(inngest_event).await {
        tracing::error!(error = %e, job_id = %created_job.id, "Failed to send Inngest event");
    }

    let response = RenderResponse {
        job: created_job.into(),
        artifact: RenderArtifactResponse {
            id: output_artifact.id,
            owner: output_artifact.owner,
            kind: output_artifact.kind,
            status: output_artifact.status,
            source: output_artifact.source,
            source_job_id: output_artifact.source_job_id,
            created_at: output_artifact.created_at,
        },
    };

    Ok((StatusCode::CREATED, Json(response)))
}
