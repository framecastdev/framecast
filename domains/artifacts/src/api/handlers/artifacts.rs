//! Artifact management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AuthUser;
use framecast_common::{Error, Result, Urn, ValidatedJson};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::ArtifactsState;
use crate::domain::entities::{ArtifactKind, ArtifactSource, ArtifactStatus};

/// Request for creating a storyboard artifact
#[derive(Debug, Deserialize, Validate)]
pub struct CreateStoryboardRequest {
    /// Storyboard spec (required)
    pub spec: serde_json::Value,

    /// Optional project ID (must be team-owned)
    pub project_id: Option<Uuid>,

    /// Owner URN (defaults to user URN)
    pub owner: Option<String>,
}

/// Artifact response DTO
#[derive(Debug, Serialize)]
pub struct ArtifactResponse {
    pub id: Uuid,
    pub owner: String,
    pub created_by: Uuid,
    pub project_id: Option<Uuid>,
    pub kind: ArtifactKind,
    pub status: ArtifactStatus,
    pub source: ArtifactSource,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub spec: Option<serde_json::Value>,
    pub conversation_id: Option<Uuid>,
    pub source_job_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<crate::domain::entities::Artifact> for ArtifactResponse {
    fn from(a: crate::domain::entities::Artifact) -> Self {
        Self {
            id: a.id,
            owner: a.owner,
            created_by: a.created_by,
            project_id: a.project_id,
            kind: a.kind,
            status: a.status,
            source: a.source,
            filename: a.filename,
            content_type: a.content_type,
            size_bytes: a.size_bytes,
            spec: a.spec,
            conversation_id: a.conversation_id,
            source_job_id: a.source_job_id,
            metadata: a.metadata.0,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

/// List artifacts for the authenticated user
pub async fn list_artifacts(
    AuthUser(ctx): AuthUser,
    State(state): State<ArtifactsState>,
) -> Result<Json<Vec<ArtifactResponse>>> {
    let owner = Urn::user(ctx.user.id);
    let artifacts = state
        .repos
        .artifacts
        .list_by_owner(&owner.to_string())
        .await?;

    let responses: Vec<ArtifactResponse> = artifacts.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Get a single artifact by ID
pub async fn get_artifact(
    AuthUser(ctx): AuthUser,
    State(state): State<ArtifactsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ArtifactResponse>> {
    let artifact = state
        .repos
        .artifacts
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Artifact not found".to_string()))?;

    // Authorization: check ownership
    let user_urn = Urn::user(ctx.user.id).to_string();
    if artifact.owner != user_urn {
        return Err(Error::NotFound("Artifact not found".to_string()));
    }

    Ok(Json(artifact.into()))
}

/// Create a storyboard artifact
pub async fn create_storyboard(
    AuthUser(ctx): AuthUser,
    State(state): State<ArtifactsState>,
    ValidatedJson(req): ValidatedJson<CreateStoryboardRequest>,
) -> Result<(StatusCode, Json<ArtifactResponse>)> {
    let owner = match req.owner {
        Some(ref urn_str) => urn_str
            .parse::<Urn>()
            .map_err(|_| Error::Validation("Invalid owner URN".to_string()))?,
        None => Urn::user(ctx.user.id),
    };

    let artifact = crate::domain::entities::Artifact::new_storyboard(
        owner,
        ctx.user.id,
        req.project_id,
        req.spec,
    )?;

    let created = state.repos.artifacts.create(&artifact).await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// Delete an artifact
pub async fn delete_artifact(
    AuthUser(ctx): AuthUser,
    State(state): State<ArtifactsState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let artifact = state
        .repos
        .artifacts
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Artifact not found".to_string()))?;

    // Authorization: check ownership
    let user_urn = Urn::user(ctx.user.id).to_string();
    if artifact.owner != user_urn {
        return Err(Error::NotFound("Artifact not found".to_string()));
    }

    state.repos.artifacts.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
