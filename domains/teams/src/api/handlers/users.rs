//! User management API handlers
//!
//! Implements user profile operations including:
//! - GET /v1/account - Get current user profile
//! - PATCH /v1/account - Update user profile
//! - POST /v1/account/upgrade - Upgrade user tier

use crate::{User, UserTier};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::{AuthUser, TeamsState};

/// Response for user profile operations
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub tier: UserTier,
    pub credits: i32,
    pub ephemeral_storage_bytes: i64,
    pub upgraded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            name: user.name,
            avatar_url: user.avatar_url,
            tier: user.tier,
            credits: user.credits,
            ephemeral_storage_bytes: user.ephemeral_storage_bytes,
            upgraded_at: user.upgraded_at,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

/// Request for updating user profile
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,

    #[validate(url)]
    pub avatar_url: Option<String>,
}

/// Request for tier upgrade
#[derive(Debug, Deserialize, Validate)]
pub struct UpgradeTierRequest {
    pub target_tier: UserTier,
}

/// Error response for API operations
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

/// Standard API error type
#[derive(Debug)]
pub enum ApiError {
    Validation(String),
    NotFound(String),
    Forbidden(String),
    Conflict(String),
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match &self {
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.clone()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.clone()),
            ApiError::Internal(e) => {
                tracing::error!(error = %e, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_SERVER_ERROR",
                    "Internal server error".to_string(),
                )
            }
        };

        let error_response = ErrorResponse {
            error: ErrorDetail {
                code: code.to_string(),
                message,
            },
        };

        (status, Json(error_response)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        ApiError::Internal(error)
    }
}

/// GET /v1/account - Get current user profile
pub async fn get_profile(
    AuthUser(auth_context): AuthUser,
    State(_state): State<TeamsState>,
) -> Result<Json<UserResponse>, ApiError> {
    let user_response = UserResponse::from(auth_context.user);
    Ok(Json(user_response))
}

/// PATCH /v1/account - Update user profile
pub async fn update_profile(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::Validation(format!("Validation failed: {}", e)))?;

    let user_id = auth_context.user.id;

    // Update user in database
    let updated_user = state
        .repos
        .users
        .update_profile(user_id, request.name, request.avatar_url)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user_id = %user_id, "Failed to update user profile");
            ApiError::Internal(anyhow::anyhow!("Failed to update profile: {}", e))
        })?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(UserResponse::from(updated_user)))
}

/// POST /v1/account/upgrade - Upgrade user tier
pub async fn upgrade_tier(
    AuthUser(auth_context): AuthUser,
    State(state): State<TeamsState>,
    Json(request): Json<UpgradeTierRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::Validation(format!("Validation failed: {}", e)))?;

    let current_user = &auth_context.user;
    let user_id = current_user.id;

    // Business logic validation
    match (&current_user.tier, &request.target_tier) {
        (UserTier::Starter, UserTier::Creator) => {
            // Valid upgrade path
        }
        (UserTier::Creator, UserTier::Starter) => {
            // Downgrade: Check if user has team memberships (INV-U3)
            if !auth_context.memberships.is_empty() {
                return Err(ApiError::Forbidden(
                    "Cannot downgrade to starter while having team memberships. Leave all teams first.".to_string(),
                ));
            }
        }
        (current, target) if current == target => {
            return Err(ApiError::Conflict(format!(
                "User is already {}",
                current.to_string().to_lowercase()
            )));
        }
        _ => {
            return Err(ApiError::Validation(
                "Invalid tier upgrade path".to_string(),
            ));
        }
    }

    // Perform tier upgrade
    let updated_user = state
        .repos
        .users
        .upgrade_tier(user_id, request.target_tier.clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user_id = %user_id, target_tier = ?request.target_tier, "Failed to upgrade user tier");
            ApiError::Internal(anyhow::anyhow!("Failed to upgrade tier: {}", e))
        })?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    tracing::info!(
        user_id = %user_id,
        from_tier = ?current_user.tier,
        to_tier = ?request.target_tier,
        "User tier upgraded successfully"
    );

    Ok(Json(UserResponse::from(updated_user)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_response_serialization() {
        let user = User {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: Some("Test User".to_string()),
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 100,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let response = UserResponse::from(user);
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("test@example.com"));
        assert!(json.contains("Test User"));
        assert!(json.contains("starter"));
    }

    #[test]
    fn test_update_profile_validation() {
        // Valid request
        let valid_request = UpdateProfileRequest {
            name: Some("Valid Name".to_string()),
            avatar_url: Some("https://example.com/avatar.jpg".to_string()),
        };
        assert!(valid_request.validate().is_ok());

        // Invalid URL
        let invalid_url_request = UpdateProfileRequest {
            name: Some("Valid Name".to_string()),
            avatar_url: Some("not-a-url".to_string()),
        };
        assert!(invalid_url_request.validate().is_err());

        // Empty name
        let empty_name_request = UpdateProfileRequest {
            name: Some("".to_string()),
            avatar_url: None,
        };
        assert!(empty_name_request.validate().is_err());
    }

    #[test]
    fn test_upgrade_tier_validation() {
        let request = UpgradeTierRequest {
            target_tier: UserTier::Creator,
        };
        assert!(request.validate().is_ok());
    }
}
