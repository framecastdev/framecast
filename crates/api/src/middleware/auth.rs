//! Authentication middleware for Framecast API
//!
//! This module provides JWT token validation for Supabase authentication
//! and context extraction for protected endpoints.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use framecast_db::repositories::Repositories;
use framecast_domain::{auth::AuthContext, entities::*};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

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

/// JWT claims from Supabase
#[derive(Debug, Serialize, Deserialize)]
pub struct SupabaseClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Email
    pub email: Option<String>,
    /// Issued at
    pub iat: u64,
    /// Expires at
    pub exp: u64,
    /// Audience
    pub aud: String,
    /// Role (authenticated user)
    pub role: String,
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub issuer: Option<String>,
    pub audience: Option<String>,
}

/// Application state containing repositories and config
#[derive(Clone)]
pub struct AppState {
    pub repos: Repositories,
    pub auth_config: AuthConfig,
}

/// Authenticated user extractor
#[derive(Debug)]
pub struct AuthUser(pub AuthContext);

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorization)?;

        let token = extract_bearer_token(auth_header)?;

        let claims = validate_jwt_token(&token, &state.auth_config)?;

        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidUserId)?;

        // Load user from database
        let user = state
            .repos
            .users
            .find(user_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, user_id = %user_id, "Failed to load user");
                AuthError::UserLoadError
            })?
            .ok_or(AuthError::UserNotFound)?;

        // Load user's team memberships (only for creators)
        let memberships = if user.tier == UserTier::Creator {
            state.repos.teams.find_by_user(user_id).await.map_err(|e| {
                tracing::error!(error = %e, user_id = %user_id, "Failed to load memberships");
                AuthError::MembershipsLoadError
            })?
        } else {
            vec![]
        };

        let auth_context = AuthContext::new(user, memberships, None);

        Ok(AuthUser(auth_context))
    }
}

/// API Key authenticated user extractor
#[derive(Debug)]
pub struct ApiKeyUser(pub AuthContext);

#[async_trait]
impl FromRequestParts<AppState> for ApiKeyUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorization)?;

        let api_key = extract_api_key(auth_header)?;

        // Authenticate using API key
        let authenticated_key = state
            .repos
            .api_keys
            .authenticate(&api_key)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to authenticate API key");
                AuthError::AuthenticationFailed
            })?
            .ok_or(AuthError::InvalidApiKey)?;

        // Load user from database
        let user = state
            .repos
            .users
            .find(authenticated_key.user_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, user_id = %authenticated_key.user_id, "Failed to load user");
                AuthError::UserLoadError
            })?
            .ok_or(AuthError::UserNotFound)?;

        // Load user's team memberships (only for creators)
        let memberships = if user.tier == UserTier::Creator {
            state
                .repos
                .teams
                .find_by_user(authenticated_key.user_id)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, user_id = %authenticated_key.user_id, "Failed to load memberships");
                    AuthError::MembershipsLoadError
                })?
        } else {
            vec![]
        };

        let auth_context = AuthContext::new(user, memberships, Some(authenticated_key));

        Ok(ApiKeyUser(auth_context))
    }
}

/// Extract bearer token from Authorization header
fn extract_bearer_token(header: &HeaderValue) -> Result<String, AuthError> {
    let header_str = header
        .to_str()
        .map_err(|_| AuthError::InvalidAuthorizationFormat)?;

    if let Some(token) = header_str.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(AuthError::InvalidAuthorizationFormat)
    }
}

/// Extract API key from Authorization header
fn extract_api_key(header: &HeaderValue) -> Result<String, AuthError> {
    let header_str = header
        .to_str()
        .map_err(|_| AuthError::InvalidAuthorizationFormat)?;

    // Support both "Bearer sk_live_..." and "sk_live_..." formats
    let api_key = if let Some(token) = header_str.strip_prefix("Bearer ") {
        token
    } else {
        header_str
    };

    if api_key.starts_with("sk_live_") {
        Ok(api_key.to_string())
    } else {
        Err(AuthError::InvalidApiKey)
    }
}

/// Validate JWT token from Supabase
fn validate_jwt_token(token: &str, config: &AuthConfig) -> Result<SupabaseClaims, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);

    if let Some(aud) = &config.audience {
        validation.set_audience(&[aud]);
    }

    if let Some(iss) = &config.issuer {
        validation.set_issuer(&[iss]);
    }

    let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_ref());

    let token_data = decode::<SupabaseClaims>(token, &decoding_key, &validation).map_err(|e| {
        tracing::debug!(error = %e, "JWT validation failed");
        AuthError::InvalidToken
    })?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_bearer_token() {
        // Valid bearer token
        let header = HeaderValue::from_static("Bearer abc123");
        let result = extract_bearer_token(&header);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "abc123");

        // Invalid format
        let header = HeaderValue::from_static("abc123");
        let result = extract_bearer_token(&header);
        assert!(result.is_err());

        // Basic auth (wrong type)
        let header = HeaderValue::from_static("Basic abc123");
        let result = extract_bearer_token(&header);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_api_key() {
        // Valid API key with Bearer prefix
        let header = HeaderValue::from_static("Bearer sk_live_abc123");
        let result = extract_api_key(&header);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "sk_live_abc123");

        // Valid API key without Bearer prefix
        let header = HeaderValue::from_static("sk_live_abc123");
        let result = extract_api_key(&header);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "sk_live_abc123");

        // Invalid API key format
        let header = HeaderValue::from_static("invalid_key");
        let result = extract_api_key(&header);
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_validation_config() {
        let config = AuthConfig {
            jwt_secret: "test_secret".to_string(),
            issuer: Some("https://example.com".to_string()),
            audience: Some("framecast".to_string()),
        };

        // Test with invalid token (this will fail due to invalid signature, which is expected)
        let result = validate_jwt_token("invalid_token", &config);
        assert!(result.is_err());
    }
}
