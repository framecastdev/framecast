//! Generation management API handlers

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, Sse},
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AnyAuth;
use framecast_common::{Error, Pagination, Result, Urn, ValidatedJson};
use framecast_inngest::InngestEvent;
use framecast_runpod::RenderRequest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::GenerationsState;
use crate::domain::entities::{
    Generation, GenerationEventType, GenerationFailureType, GenerationStatus,
};
use crate::repository::transactions::{
    count_active_for_owner_tx, create_generation_event_tx, create_generation_tx,
};

/// Generation response DTO
#[derive(Debug, Serialize)]
pub struct GenerationResponse {
    pub id: Uuid,
    pub owner: String,
    pub triggered_by: Uuid,
    pub project_id: Option<Uuid>,
    pub status: GenerationStatus,
    pub spec_snapshot: serde_json::Value,
    pub options: serde_json::Value,
    pub progress: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<serde_json::Value>,
    pub credits_charged: i32,
    pub failure_type: Option<GenerationFailureType>,
    pub credits_refunded: i32,
    pub idempotency_key: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Generation> for GenerationResponse {
    fn from(g: Generation) -> Self {
        Self {
            id: g.id,
            owner: g.owner,
            triggered_by: g.triggered_by,
            project_id: g.project_id,
            status: g.status,
            spec_snapshot: g.spec_snapshot.0,
            options: g.options.0,
            progress: g.progress.0,
            output: g.output.map(|o| o.0),
            output_size_bytes: g.output_size_bytes,
            error: g.error.map(|e| e.0),
            credits_charged: g.credits_charged,
            failure_type: g.failure_type,
            credits_refunded: g.credits_refunded,
            idempotency_key: g.idempotency_key,
            started_at: g.started_at,
            completed_at: g.completed_at,
            created_at: g.created_at,
            updated_at: g.updated_at,
        }
    }
}

/// Query parameters for listing generations
#[derive(Debug, Deserialize)]
pub struct ListGenerationsParams {
    pub status: Option<GenerationStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Unified request for creating a generation.
///
/// Two modes:
/// - **From existing artifact**: provide `artifact_id` (and optionally `options`)
/// - **From raw spec (ephemeral)**: provide `spec` (and optionally `owner`, `options`, `idempotency_key`)
#[derive(Debug, Deserialize, Validate)]
pub struct CreateGenerationRequest {
    /// Mode 1: generate from an existing artifact
    pub artifact_id: Option<Uuid>,
    /// Mode 2: ephemeral generation from a raw spec
    pub spec: Option<serde_json::Value>,
    pub owner: Option<String>,
    pub options: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
}

/// Request for cloning a generation
#[derive(Debug, Deserialize)]
pub struct CloneGenerationRequest {
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
    pub source_generation_id: Option<Uuid>,
    pub filename: Option<String>,
    pub s3_key: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Lightweight artifact response for generation-from-artifact endpoint
#[derive(Debug, Serialize)]
pub struct GenerationArtifactResponse {
    pub id: Uuid,
    pub owner: String,
    pub kind: String,
    pub status: String,
    pub source: String,
    pub source_generation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Combined response when generating from an artifact
#[derive(Debug, Serialize)]
pub struct GenerationWithArtifactResponse {
    pub generation: GenerationResponse,
    pub artifact: GenerationArtifactResponse,
}

/// List generations for the authenticated user (personal + team-owned)
pub async fn list_generations(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    Query(params): Query<ListGenerationsParams>,
) -> Result<Json<Vec<GenerationResponse>>> {
    let owner_urns = ctx.accessible_owner_urns();

    let pagination = Pagination {
        offset: params.offset,
        limit: params.limit,
    };
    let generations = state
        .repos
        .generations
        .list_by_owners(
            &owner_urns,
            params.status.as_ref(),
            pagination.limit(),
            pagination.offset(),
        )
        .await?;

    let responses: Vec<GenerationResponse> = generations.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Get a single generation by ID
pub async fn get_generation(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GenerationResponse>> {
    let generation = state
        .repos
        .generations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Generation not found".to_string()))?;

    // Authorization: check ownership (personal or team membership)
    let owner_urn = generation
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on generation".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Generation not found".to_string()));
    }

    Ok(Json(generation.into()))
}

/// Create a generation — unified endpoint handling two modes:
///
/// 1. **From artifact**: `artifact_id` in body → looks up artifact, creates generation + output artifact
/// 2. **Ephemeral**: `spec` in body → creates generation only (not tied to an artifact)
pub async fn create_generation(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    ValidatedJson(req): ValidatedJson<CreateGenerationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    match (req.artifact_id, req.spec.clone()) {
        (Some(artifact_id), None) => {
            create_generation_from_artifact(ctx, state, artifact_id, req).await
        }
        (None, Some(_)) => create_ephemeral_generation(ctx, state, req).await,
        (Some(_), Some(_)) => Err(Error::Validation(
            "Provide either 'artifact_id' or 'spec', not both".to_string(),
        )),
        (None, None) => Err(Error::Validation(
            "Provide either 'artifact_id' or 'spec'".to_string(),
        )),
    }
}

/// Create an ephemeral generation (not tied to an artifact)
async fn create_ephemeral_generation(
    ctx: framecast_auth::AuthContext,
    state: GenerationsState,
    req: CreateGenerationRequest,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let spec = req.spec.unwrap_or_default();

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
                    "Starter users cannot create generations for teams".to_string(),
                ));
            }
            urn
        }
        None => Urn::user(ctx.user.id),
    };

    // Check idempotency
    if let Some(ref key) = req.idempotency_key {
        if let Some(existing) = state
            .repos
            .generations
            .find_by_idempotency_key(ctx.user.id, key)
            .await?
        {
            let resp: GenerationResponse = existing.into();
            return Ok((StatusCode::OK, Json(serde_json::to_value(resp)?)));
        }
    }

    // Create generation
    let generation = Generation::new(
        owner,
        ctx.user.id,
        None,
        spec.clone(),
        req.options.clone(),
        0, // credits deferred
        req.idempotency_key,
    )?;

    // Transaction: check concurrency (FOR UPDATE lock) + create generation + initial event
    let mut tx = state.repos.begin().await?;
    let active_count = count_active_for_owner_tx(&mut tx, &generation.owner).await?;
    let max_concurrent = if ctx.is_starter() { 1 } else { 5 };
    if active_count >= max_concurrent {
        return Err(Error::Conflict(format!(
            "Concurrency limit exceeded: {} active generations (max {})",
            active_count, max_concurrent
        )));
    }
    let created = create_generation_tx(&mut tx, &generation).await?;
    create_generation_event_tx(
        &mut tx,
        created.id,
        1,
        GenerationEventType::Queued,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;

    // Send Inngest event
    let inngest_event = InngestEvent {
        name: "framecast/generation.queued".to_string(),
        data: serde_json::json!({
            "generation_id": created.id,
            "owner": created.owner,
            "spec_snapshot": spec,
            "options": req.options.unwrap_or_else(|| serde_json::json!({}))
        }),
        user: None,
        id: None,
        ts: None,
    };
    if let Err(e) = state.inngest.send_event(inngest_event).await {
        tracing::error!(error = %e, generation_id = %created.id, "Failed to send Inngest event");
    }

    let resp: GenerationResponse = created.into();
    Ok((StatusCode::CREATED, Json(serde_json::to_value(resp)?)))
}

/// Create a generation from an existing artifact (creates generation + output artifact)
async fn create_generation_from_artifact(
    ctx: framecast_auth::AuthContext,
    state: GenerationsState,
    artifact_id: Uuid,
    req: CreateGenerationRequest,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    // CQRS read: query artifact directly from artifacts table
    let artifact: ArtifactReadModel = sqlx::query_as::<_, ArtifactReadModel>(
        r#"
        SELECT id, owner, created_by, project_id, kind::text as kind, status::text as status,
               source::text as source, spec, source_generation_id, filename, s3_key, content_type,
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

    // Create Generation from the source artifact's spec
    let spec = artifact
        .spec
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let generation = Generation::new(
        owner_urn.clone(),
        ctx.user.id,
        None,
        spec,
        req.options.clone(),
        0, // credits deferred
        None,
    )?;

    // Transaction: check concurrency (FOR UPDATE lock) + create generation + artifact + event
    let mut tx = state.repos.begin().await?;
    let active_count = count_active_for_owner_tx(&mut tx, &generation.owner).await?;
    let max_concurrent = if ctx.is_starter() { 1 } else { 5 };
    if active_count >= max_concurrent {
        return Err(Error::Conflict(format!(
            "Concurrency limit exceeded: {} active generations (max {})",
            active_count, max_concurrent
        )));
    }

    let created = create_generation_tx(&mut tx, &generation).await?;

    // CQRS write: INSERT directly into artifacts table
    let output_artifact_id = Uuid::new_v4();
    let now = Utc::now();
    let output_artifact: ArtifactReadModel = sqlx::query_as::<_, ArtifactReadModel>(
        r#"
        INSERT INTO artifacts (
            id, owner, created_by, project_id,
            kind, status, source,
            filename, s3_key, content_type, size_bytes,
            spec, conversation_id, source_generation_id,
            metadata, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5::artifact_kind, 'pending'::asset_status, 'generation'::artifact_source,
                $6, $7, $8, $9, $10, NULL, $11, '{}'::jsonb, $12, $13)
        RETURNING id, owner, created_by, project_id, kind::text as kind, status::text as status,
                  source::text as source, spec, source_generation_id, filename, s3_key, content_type,
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
    .bind(1_i64) // placeholder; updated on generation completion via callback
    .bind(&artifact.spec)
    .bind(created.id)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    create_generation_event_tx(
        &mut tx,
        created.id,
        1,
        GenerationEventType::Queued,
        serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    // Submit to render service
    let render_request = RenderRequest {
        generation_id: created.id,
        spec_snapshot: created.spec_snapshot.0.clone(),
        options: created.options.0.clone(),
        callback_url: format!("{}/internal/generations/callback", state.callback_base_url),
    };
    if let Err(e) = state.render.submit_render(render_request).await {
        tracing::error!(error = %e, generation_id = %created.id, "Failed to submit render request");
    }

    // Send Inngest event
    let inngest_event = InngestEvent {
        name: "framecast/generation.queued".to_string(),
        data: serde_json::json!({
            "generation_id": created.id,
            "owner": created.owner,
            "spec_snapshot": created.spec_snapshot.0,
            "options": created.options.0
        }),
        user: None,
        id: None,
        ts: None,
    };
    if let Err(e) = state.inngest.send_event(inngest_event).await {
        tracing::error!(error = %e, generation_id = %created.id, "Failed to send Inngest event");
    }

    let response = GenerationWithArtifactResponse {
        generation: created.into(),
        artifact: GenerationArtifactResponse {
            id: output_artifact.id,
            owner: output_artifact.owner,
            kind: output_artifact.kind,
            status: output_artifact.status,
            source: output_artifact.source,
            source_generation_id: output_artifact.source_generation_id,
            created_at: output_artifact.created_at,
        },
    };

    Ok((StatusCode::CREATED, Json(serde_json::to_value(response)?)))
}

/// Cancel a generation
pub async fn cancel_generation(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GenerationResponse>> {
    let mut generation = state
        .repos
        .generations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Generation not found".to_string()))?;

    // Authorization: triggered_by matches OR team owner/admin role
    let owner_urn = generation
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on generation".to_string()))?;

    let authorized = if generation.triggered_by == ctx.user.id {
        true
    } else if let Ok(framecast_common::UrnComponents::Team { team_id }) = owner_urn.parse() {
        ctx.get_team_role(team_id)
            .map(|role| role.can_admin())
            .unwrap_or(false)
    } else {
        false
    };

    if !authorized {
        return Err(Error::NotFound("Generation not found".to_string()));
    }

    // Check if terminal
    if generation.is_terminal() {
        return Err(Error::Conflict(format!(
            "Generation is already in terminal state '{:?}'",
            generation.status
        )));
    }

    // Cancel generation (state machine transition)
    generation.cancel()?;

    // Update repo
    let updated = state.repos.generations.update(&generation).await?;

    // Create generation event
    let next_seq = state.repos.generation_events.next_sequence(id).await?;
    state
        .repos
        .generation_events
        .create(
            id,
            next_seq,
            GenerationEventType::Canceled,
            serde_json::json!({}),
        )
        .await?;

    Ok(Json(updated.into()))
}

/// Delete a generation (only ephemeral + terminal generations)
pub async fn delete_generation(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let generation = state
        .repos
        .generations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Generation not found".to_string()))?;

    // Authorization: check ownership
    let owner_urn = generation
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on generation".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Generation not found".to_string()));
    }

    // Must be ephemeral (no project) and terminal
    if !generation.is_ephemeral() {
        return Err(Error::Validation(
            "Only ephemeral generations (without project) can be deleted".to_string(),
        ));
    }
    if !generation.is_terminal() {
        return Err(Error::Validation(
            "Only terminal generations can be deleted".to_string(),
        ));
    }

    state.repos.generations.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clone a generation (creates a new generation from an existing terminal generation)
pub async fn clone_generation(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    Path(id): Path<Uuid>,
    body: Option<Json<CloneGenerationRequest>>,
) -> Result<(StatusCode, Json<GenerationResponse>)> {
    let original = state
        .repos
        .generations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Generation not found".to_string()))?;

    // Authorization: check ownership of original
    let owner_urn = original
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on generation".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Generation not found".to_string()));
    }

    // Original must be terminal
    if !original.is_terminal() {
        return Err(Error::Validation(
            "Cannot clone an active generation; generation must be in a terminal state".to_string(),
        ));
    }

    // Resolve owner for new generation
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

    // Create new generation from original's spec and options
    let new_generation = Generation::new(
        new_owner,
        ctx.user.id,
        None,
        original.spec_snapshot.0,
        Some(original.options.0.clone()),
        0, // credits deferred
        None,
    )?;

    // Transaction: check concurrency (FOR UPDATE lock) + create generation + initial event
    let mut tx = state.repos.begin().await?;
    let active_count = count_active_for_owner_tx(&mut tx, &new_generation.owner).await?;
    let max_concurrent = if ctx.is_starter() { 1 } else { 5 };
    if active_count >= max_concurrent {
        return Err(Error::Conflict(format!(
            "Concurrency limit exceeded: {} active generations (max {})",
            active_count, max_concurrent
        )));
    }
    let created = create_generation_tx(&mut tx, &new_generation).await?;
    create_generation_event_tx(
        &mut tx,
        created.id,
        1,
        GenerationEventType::Queued,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;

    // Send Inngest event
    let inngest_event = InngestEvent {
        name: "framecast/generation.queued".to_string(),
        data: serde_json::json!({
            "generation_id": created.id,
            "owner": created.owner,
            "spec_snapshot": created.spec_snapshot.0,
            "options": created.options.0
        }),
        user: None,
        id: None,
        ts: None,
    };
    if let Err(e) = state.inngest.send_event(inngest_event).await {
        tracing::error!(error = %e, generation_id = %created.id, "Failed to send Inngest event");
    }

    Ok((StatusCode::CREATED, Json(created.into())))
}

/// Get generation events (SSE stream)
pub async fn get_generation_events(
    AnyAuth(ctx): AnyAuth,
    State(state): State<GenerationsState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<
    Sse<impl futures_core::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    let generation = state
        .repos
        .generations
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Generation not found".to_string()))?;

    // Authorization: check ownership
    let owner_urn = generation
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on generation".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Generation not found".to_string()));
    }

    // Parse Last-Event-ID header to get last seen sequence
    let after_sequence = headers
        .get("Last-Event-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            // Format: {generation_id}:{sequence}
            v.rsplit(':').next().and_then(|s| s.parse::<i64>().ok())
        });

    let generation_id = id;
    let repos = state.repos.clone();

    let stream = async_stream::stream! {
        let mut last_seq = after_sequence.unwrap_or(0);
        let mut iterations: u32 = 0;
        // 15-minute maximum duration at 1s intervals prevents resource leak from stuck generations
        const MAX_ITERATIONS: u32 = 900;

        loop {
            // Query for new events
            let events = match repos.generation_events.list_by_generation(generation_id, Some(last_seq)).await {
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
                    .id(format!("{}:{}", generation_id, event.sequence))
                    .event(event_type_str)
                    .data(data);

                yield Ok(sse_event);
                last_seq = event.sequence;
            }

            // Check if generation is terminal — if so, close the stream
            let current = match repos.generations.find(generation_id).await {
                Ok(Some(g)) => g,
                _ => break,
            };
            if current.is_terminal() {
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
