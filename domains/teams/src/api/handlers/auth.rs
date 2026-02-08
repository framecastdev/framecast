//! Auth introspection API handler
//!
//! Implements:
//! - GET /v1/auth/whoami — Return authentication context for the current caller

use crate::api::handlers::users::UserResponse;
use crate::api::middleware::TeamsState;
use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use framecast_auth::AnyAuth;
use framecast_common::{Error, Result};
use serde::Serialize;
use uuid::Uuid;

/// Subset of API key metadata relevant for auth introspection.
///
/// Excludes fields that are not useful in this context (`user_id`,
/// `revoked_at`, `last_used_at`, `created_at`).
#[derive(Debug, Serialize)]
pub struct WhoamiApiKeyInfo {
    pub id: Uuid,
    pub owner: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response shape for `GET /v1/auth/whoami`
#[derive(Debug, Serialize)]
pub struct WhoamiResponse {
    pub auth_method: &'static str,
    pub user: UserResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<WhoamiApiKeyInfo>,
}

/// GET /v1/auth/whoami — Return authentication context for the current caller
pub async fn whoami(
    AnyAuth(auth_context): AnyAuth,
    State(state): State<TeamsState>,
) -> Result<Json<WhoamiResponse>> {
    let api_key_info = auth_context.api_key.as_ref().map(|key| WhoamiApiKeyInfo {
        id: key.id,
        owner: key.owner.clone(),
        name: key.name.clone(),
        key_prefix: key.key_prefix.clone(),
        scopes: key.scopes.clone(),
        expires_at: key.expires_at,
    });

    let auth_method = if api_key_info.is_some() {
        "api_key"
    } else {
        "jwt"
    };

    // Load full user for UserResponse (needs credits, storage, upgraded_at)
    let user = state
        .repos
        .users
        .find(auth_context.user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to load user: {}", e)))?
        .ok_or_else(|| Error::NotFound("User not found".to_string()))?;

    let response = WhoamiResponse {
        auth_method,
        user: UserResponse::from(user),
        api_key: api_key_info,
    };

    Ok(Json(response))
}
