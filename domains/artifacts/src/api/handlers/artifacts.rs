//! Artifact management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AnyAuth;
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
    pub s3_key: Option<String>,
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
            s3_key: a.s3_key,
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

/// List artifacts for the authenticated user (personal + team-owned)
pub async fn list_artifacts(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ArtifactsState>,
) -> Result<Json<Vec<ArtifactResponse>>> {
    // Collect all owner URNs the user can access: personal + each team
    let mut owner_urns = vec![Urn::user(ctx.user.id).to_string()];
    for membership in &ctx.memberships {
        owner_urns.push(Urn::team(membership.team_id).to_string());
    }

    let artifacts = state.repos.artifacts.list_by_owners(&owner_urns).await?;

    let responses: Vec<ArtifactResponse> = artifacts.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Get a single artifact by ID
pub async fn get_artifact(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ArtifactsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ArtifactResponse>> {
    let artifact = state
        .repos
        .artifacts
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Artifact not found".to_string()))?;

    // Authorization: check ownership (personal or team membership)
    let owner_urn = artifact
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on artifact".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Artifact not found".to_string()));
    }

    Ok(Json(artifact.into()))
}

/// Create a storyboard artifact
pub async fn create_storyboard(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ArtifactsState>,
    ValidatedJson(req): ValidatedJson<CreateStoryboardRequest>,
) -> Result<(StatusCode, Json<ArtifactResponse>)> {
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
            urn
        }
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

/// Request for creating a character artifact
#[derive(Debug, Deserialize, Validate)]
pub struct CreateCharacterRequest {
    /// Character spec (must contain non-empty "prompt")
    pub spec: serde_json::Value,

    /// Artifact source (defaults to Upload)
    #[serde(default)]
    pub source: Option<ArtifactSource>,

    /// Optional conversation ID (required if source=conversation)
    pub conversation_id: Option<Uuid>,

    /// Optional project ID (must be team-owned)
    pub project_id: Option<Uuid>,

    /// Owner URN (defaults to user URN)
    pub owner: Option<String>,
}

/// Create a character artifact
pub async fn create_character(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ArtifactsState>,
    ValidatedJson(req): ValidatedJson<CreateCharacterRequest>,
) -> Result<(StatusCode, Json<ArtifactResponse>)> {
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
            urn
        }
        None => Urn::user(ctx.user.id),
    };

    let source = req.source.unwrap_or(ArtifactSource::Upload);

    let artifact = crate::domain::entities::Artifact::new_character(
        owner,
        ctx.user.id,
        req.project_id,
        req.spec,
        source,
        req.conversation_id,
    )?;

    let created = state.repos.artifacts.create(&artifact).await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// Render an artifact (currently only characters → creates a pending image artifact)
pub async fn render_artifact(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ArtifactsState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<ArtifactResponse>)> {
    let artifact = state
        .repos
        .artifacts
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Artifact not found".to_string()))?;

    // Authorization: check ownership (personal or team membership)
    let owner_urn = artifact
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on artifact".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Artifact not found".to_string()));
    }

    // Only character artifacts can be rendered
    if artifact.kind != ArtifactKind::Character {
        return Err(Error::Validation(format!(
            "Only character artifacts can be rendered, got '{}'",
            artifact.kind
        )));
    }

    // Create a new pending image artifact from the character
    let owner_urn = artifact
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on artifact".to_string()))?;

    let image_artifact = crate::domain::entities::Artifact::new_media(
        owner_urn,
        ctx.user.id,
        artifact.project_id,
        ArtifactKind::Image,
        format!("render-{}.png", artifact.id),
        format!("renders/{}/{}.png", ctx.user.id, Uuid::new_v4()),
        "image/png".to_string(),
        1, // placeholder size — will be updated when rendering completes
    )?;

    let created = state.repos.artifacts.create(&image_artifact).await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// Delete an artifact
pub async fn delete_artifact(
    AnyAuth(ctx): AnyAuth,
    State(state): State<ArtifactsState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let artifact = state
        .repos
        .artifacts
        .find(id)
        .await?
        .ok_or_else(|| Error::NotFound("Artifact not found".to_string()))?;

    // Authorization: check ownership (personal or team membership)
    let owner_urn = artifact
        .owner
        .parse::<Urn>()
        .map_err(|_| Error::Internal("Invalid owner URN on artifact".to_string()))?;
    if !ctx.can_access_urn(&owner_urn) {
        return Err(Error::NotFound("Artifact not found".to_string()));
    }

    state.repos.artifacts.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
