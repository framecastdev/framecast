//! API Key management handlers
//!
//! Implements API key CRUD operations:
//! - GET /v1/auth/keys         — List user's API keys
//! - GET /v1/auth/keys/{id}    — Get single API key
//! - POST /v1/auth/keys        — Create new API key
//! - PATCH /v1/auth/keys/{id}  — Update API key name
//! - DELETE /v1/auth/keys/{id} — Revoke API key

use crate::domain::entities::{ApiKey, AuthenticatedApiKey};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use framecast_auth::{AuthContext, AuthTier, AuthUser};
use framecast_common::{Error, Result, Urn, UrnComponents, ValidatedJson};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::TeamsState;

// ============================================================
// Scope constants
// ============================================================

/// All scopes allowed by the spec (§9.3)
pub const ALLOWED_SCOPES: &[&str] = &[
    "generate",
    "generations:read",
    "generations:write",
    "assets:read",
    "assets:write",
    "projects:read",
    "projects:write",
    "team:read",
    "team:admin",
    "webhooks:read",
    "webhooks:write",
    "*",
];

/// Scopes available to Starter-tier users
pub const STARTER_ALLOWED_SCOPES: &[&str] = &[
    "generate",
    "generations:read",
    "generations:write",
    "assets:read",
    "assets:write",
];

// ============================================================
// DTOs
// ============================================================

/// API key response — never exposes `key_hash`
#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub owner: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<AuthenticatedApiKey> for ApiKeyResponse {
    fn from(key: AuthenticatedApiKey) -> Self {
        Self {
            id: key.id,
            user_id: key.user_id,
            owner: key.owner,
            name: key.name,
            key_prefix: key.key_prefix,
            scopes: key.scopes,
            last_used_at: key.last_used_at,
            expires_at: key.expires_at,
            revoked_at: key.revoked_at,
            created_at: key.created_at,
        }
    }
}

/// Response for API key creation — includes the raw key (only visible once)
#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub api_key: ApiKeyResponse,
    pub raw_key: String,
}

/// Request to create a new API key
#[derive(Debug, Deserialize, Validate)]
pub struct CreateApiKeyRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub owner: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Request to update an API key
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateApiKeyRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
}

// ============================================================
// Handlers
// ============================================================

/// GET /v1/auth/keys — List all API keys for the authenticated user
pub async fn list_api_keys(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
) -> Result<Json<Vec<ApiKeyResponse>>> {
    let keys = state
        .repos
        .api_keys
        .list_by_user(auth_context.user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to list API keys: {}", e)))?;

    Ok(Json(keys.into_iter().map(ApiKeyResponse::from).collect()))
}

/// GET /v1/auth/keys/{id} — Get a single API key
pub async fn get_api_key(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
    Path(key_id): Path<Uuid>,
) -> Result<Json<ApiKeyResponse>> {
    let key = state
        .repos
        .api_keys
        .get_by_id(key_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get API key: {}", e)))?
        .ok_or_else(|| Error::NotFound("API key not found".to_string()))?;

    // Ownership check (return 404 to prevent info leak)
    if key.user_id != auth_context.user.id {
        return Err(Error::NotFound("API key not found".to_string()));
    }

    Ok(Json(ApiKeyResponse::from(key)))
}

/// POST /v1/auth/keys — Create a new API key
pub async fn create_api_key(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
    ValidatedJson(request): ValidatedJson<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>)> {
    let user = &auth_context.user;

    // Validate expires_at is in the future
    if let Some(expires_at) = request.expires_at {
        if expires_at <= Utc::now() {
            return Err(Error::Validation(
                "expires_at must be in the future".to_string(),
            ));
        }
    }

    // Resolve owner URN
    let owner_urn = match &request.owner {
        Some(owner_str) => {
            let urn: Urn = owner_str
                .parse()
                .map_err(|_| Error::Validation("Invalid owner URN format".to_string()))?;

            validate_owner_urn(&auth_context, &urn)?;
            urn
        }
        None => Urn::user(user.id),
    };

    // Resolve default scopes before validation so omitting scopes
    // doesn't bypass tier checks (Starter users cannot get "*" scope).
    let scopes = request.scopes.unwrap_or_else(|| vec!["*".to_string()]);
    validate_scopes(&scopes, &user.tier)?;

    // Create key entity
    let (api_key, raw_key) = ApiKey::new(
        user.id,
        owner_urn,
        request.name,
        Some(scopes),
        request.expires_at,
    )?;

    // Persist
    let created = state
        .repos
        .api_keys
        .create(&api_key)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create API key: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            api_key: ApiKeyResponse::from(created),
            raw_key,
        }),
    ))
}

/// PATCH /v1/auth/keys/{id} — Update an API key's name
pub async fn update_api_key(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
    Path(key_id): Path<Uuid>,
    ValidatedJson(request): ValidatedJson<UpdateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>> {
    // Check ownership first
    let existing = state
        .repos
        .api_keys
        .get_by_id(key_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to find API key: {}", e)))?
        .ok_or_else(|| Error::NotFound("API key not found".to_string()))?;

    if existing.user_id != auth_context.user.id {
        return Err(Error::NotFound("API key not found".to_string()));
    }

    // update_name returns None when revoked_at IS NOT NULL
    let updated = state
        .repos
        .api_keys
        .update_name(key_id, &request.name)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update API key: {}", e)))?
        .ok_or_else(|| Error::Conflict("Cannot update a revoked API key".to_string()))?;

    Ok(Json(ApiKeyResponse::from(updated)))
}

/// DELETE /v1/auth/keys/{id} — Revoke an API key
pub async fn revoke_api_key(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
    Path(key_id): Path<Uuid>,
) -> Result<StatusCode> {
    // Check ownership first
    let existing = state
        .repos
        .api_keys
        .get_by_id(key_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to find API key: {}", e)))?
        .ok_or_else(|| Error::NotFound("API key not found".to_string()))?;

    if existing.user_id != auth_context.user.id {
        return Err(Error::NotFound("API key not found".to_string()));
    }

    if existing.revoked_at.is_some() {
        return Err(Error::Conflict("API key is already revoked".to_string()));
    }

    state
        .repos
        .api_keys
        .revoke(key_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to revoke API key: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// Validation helpers
// ============================================================

/// Validate owner URN per invariants INV-A6, INV-A7, INV-X4
fn validate_owner_urn(auth_context: &AuthContext, urn: &Urn) -> Result<()> {
    let user = &auth_context.user;

    match urn.parse()? {
        UrnComponents::User { user_id } => {
            // Must be the user's own URN
            if user_id != user.id {
                return Err(Error::Authorization(
                    "Cannot create API key for another user".to_string(),
                ));
            }
        }
        UrnComponents::Team { .. } => {
            // INV-A7: Team URN requires creator tier
            if user.tier != AuthTier::Creator {
                return Err(Error::Authorization(
                    "Team-scoped API keys require creator tier".to_string(),
                ));
            }
            // Must have access to the team
            if !auth_context.can_access_urn(urn) {
                return Err(Error::Authorization(
                    "You are not a member of this team".to_string(),
                ));
            }
        }
        UrnComponents::TeamUser {
            team_id, user_id, ..
        } => {
            // INV-X4: Membership URN requires valid membership
            if user_id != user.id {
                return Err(Error::Authorization(
                    "Cannot create API key for another user's membership".to_string(),
                ));
            }
            if user.tier != AuthTier::Creator {
                return Err(Error::Authorization(
                    "Membership-scoped API keys require creator tier".to_string(),
                ));
            }
            // Verify membership exists
            if auth_context.get_team_role(team_id).is_none() {
                return Err(Error::Authorization(
                    "You are not a member of this team".to_string(),
                ));
            }
        }
        UrnComponents::Artifact { .. } => {
            return Err(Error::Validation(
                "Artifact URNs cannot be used as API key owners".to_string(),
            ));
        }
        UrnComponents::System { .. } => {
            return Err(Error::Validation(
                "System URNs cannot be used as API key owners".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate scopes against tier restrictions
fn validate_scopes(scopes: &[String], tier: &AuthTier) -> Result<()> {
    if scopes.is_empty() {
        return Err(Error::Validation("Scopes cannot be empty".to_string()));
    }
    for scope in scopes {
        // Check against full allowed list
        if !ALLOWED_SCOPES.contains(&scope.as_str()) {
            return Err(Error::Validation(format!("Invalid scope: {}", scope)));
        }

        // Check tier restriction for starters
        if *tier == AuthTier::Starter && !STARTER_ALLOWED_SCOPES.contains(&scope.as_str()) {
            return Err(Error::Authorization(format!(
                "Scope '{}' is not available for starter tier",
                scope
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_response_no_key_hash() {
        let key = AuthenticatedApiKey {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            owner: "framecast:user:00000000-0000-0000-0000-000000000001".to_string(),
            name: "Test Key".to_string(),
            key_prefix: "sk_live_".to_string(),
            scopes: vec!["*".to_string()],
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        };

        let response = ApiKeyResponse::from(key);
        let json = serde_json::to_string(&response).unwrap();

        assert!(!json.contains("key_hash"));
        assert!(json.contains("Test Key"));
        assert!(json.contains("sk_live_"));
    }

    #[test]
    fn test_create_request_name_too_long() {
        let request = CreateApiKeyRequest {
            name: Some("a".repeat(101)),
            owner: None,
            scopes: None,
            expires_at: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_create_request_empty_name_rejected() {
        let request = CreateApiKeyRequest {
            name: Some("".to_string()),
            owner: None,
            scopes: None,
            expires_at: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_create_request_valid_name() {
        let request = CreateApiKeyRequest {
            name: Some("My Key".to_string()),
            owner: None,
            scopes: None,
            expires_at: None,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_create_request_none_name_valid() {
        let request = CreateApiKeyRequest {
            name: None,
            owner: None,
            scopes: None,
            expires_at: None,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_update_request_empty_name_rejected() {
        let request = UpdateApiKeyRequest {
            name: "".to_string(),
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_update_request_name_too_long() {
        let request = UpdateApiKeyRequest {
            name: "a".repeat(101),
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_update_request_valid() {
        let request = UpdateApiKeyRequest {
            name: "Updated Name".to_string(),
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_starter_allowed_scopes_subset_of_allowed() {
        for scope in STARTER_ALLOWED_SCOPES {
            assert!(
                ALLOWED_SCOPES.contains(scope),
                "Starter scope '{}' not in ALLOWED_SCOPES",
                scope
            );
        }
    }

    #[test]
    fn test_validate_scopes_rejects_unknown() {
        let result = validate_scopes(&["unknown:scope".to_string()], &AuthTier::Creator);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_scopes_starter_restricted() {
        // team:read is not in STARTER_ALLOWED_SCOPES
        let result = validate_scopes(&["team:read".to_string()], &AuthTier::Starter);
        assert!(result.is_err());

        // team:admin is not in STARTER_ALLOWED_SCOPES
        let result = validate_scopes(&["team:admin".to_string()], &AuthTier::Starter);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_scopes_starter_allowed() {
        let result = validate_scopes(&["generate".to_string()], &AuthTier::Starter);
        assert!(result.is_ok());

        let result = validate_scopes(&["generations:read".to_string()], &AuthTier::Starter);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_scopes_creator_all_allowed() {
        for scope in ALLOWED_SCOPES {
            let result = validate_scopes(&[scope.to_string()], &AuthTier::Creator);
            assert!(
                result.is_ok(),
                "Creator should be allowed scope '{}'",
                scope
            );
        }
    }

    #[test]
    fn test_validate_scopes_wildcard() {
        let result = validate_scopes(&["*".to_string()], &AuthTier::Creator);
        assert!(result.is_ok());
    }
}
