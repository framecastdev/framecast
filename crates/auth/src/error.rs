//! Authentication errors

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Authentication error
#[derive(Debug)]
pub enum AuthError {
    MissingAuthorization,
    InvalidAuthorizationFormat,
    InvalidToken,
    InvalidApiKey,
    UserNotFound,
    UserLoadError,
    MembershipsLoadError,
    AuthenticationFailed,
    InvalidUserId,
    /// User tier insufficient for this operation (spec 9.1)
    InsufficientTier,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AuthError::MissingAuthorization => (
                StatusCode::UNAUTHORIZED,
                "MISSING_AUTHORIZATION",
                "Authorization header required",
            ),
            AuthError::InvalidAuthorizationFormat => (
                StatusCode::UNAUTHORIZED,
                "INVALID_AUTHORIZATION",
                "Invalid authorization header format",
            ),
            AuthError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "INVALID_TOKEN",
                "Invalid or expired token",
            ),
            AuthError::InvalidApiKey => (
                StatusCode::UNAUTHORIZED,
                "INVALID_API_KEY",
                "Invalid or expired API key",
            ),
            AuthError::UserNotFound => {
                (StatusCode::UNAUTHORIZED, "USER_NOT_FOUND", "User not found")
            }
            AuthError::UserLoadError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "USER_LOAD_ERROR",
                "Failed to load user",
            ),
            AuthError::MembershipsLoadError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "MEMBERSHIPS_LOAD_ERROR",
                "Failed to load user memberships",
            ),
            AuthError::AuthenticationFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "AUTH_ERROR",
                "Authentication failed",
            ),
            AuthError::InvalidUserId => (
                StatusCode::UNAUTHORIZED,
                "INVALID_TOKEN",
                "Invalid user ID in token",
            ),
            AuthError::InsufficientTier => (
                StatusCode::FORBIDDEN,
                "INSUFFICIENT_TIER",
                "Only creator tier users can access team operations",
            ),
        };

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}
