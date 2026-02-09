//! System asset API handlers

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::AuthUser;
use framecast_common::{Error, Result};
use serde::Serialize;

use crate::api::middleware::ArtifactsState;
use crate::domain::entities::SystemAssetCategory;

/// System asset response DTO
#[derive(Debug, Serialize)]
pub struct SystemAssetResponse {
    pub id: String,
    pub category: SystemAssetCategory,
    pub name: String,
    pub description: String,
    pub duration_seconds: Option<rust_decimal::Decimal>,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl From<crate::domain::entities::SystemAsset> for SystemAssetResponse {
    fn from(sa: crate::domain::entities::SystemAsset) -> Self {
        Self {
            id: sa.id,
            category: sa.category,
            name: sa.name,
            description: sa.description,
            duration_seconds: sa.duration_seconds,
            s3_key: sa.s3_key,
            content_type: sa.content_type,
            size_bytes: sa.size_bytes,
            tags: sa.tags.0,
            created_at: sa.created_at,
        }
    }
}

/// List all system assets
pub async fn list_system_assets(
    AuthUser(_ctx): AuthUser,
    State(state): State<ArtifactsState>,
) -> Result<Json<Vec<SystemAssetResponse>>> {
    let assets = state.repos.system_assets.list().await?;
    let responses: Vec<SystemAssetResponse> = assets.into_iter().map(Into::into).collect();
    Ok(Json(responses))
}

/// Get a single system asset by ID
pub async fn get_system_asset(
    AuthUser(_ctx): AuthUser,
    State(state): State<ArtifactsState>,
    Path(id): Path<String>,
) -> Result<Json<SystemAssetResponse>> {
    let asset = state
        .repos
        .system_assets
        .find(&id)
        .await?
        .ok_or_else(|| Error::NotFound("System asset not found".to_string()))?;

    Ok(Json(asset.into()))
}
