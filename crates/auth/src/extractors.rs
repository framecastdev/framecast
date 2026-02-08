//! Axum extractors for authentication
//!
//! Generic over any state `S` where `AuthBackend: FromRef<S>`.
//! This is axum's idiomatic nested-state pattern.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};

use crate::backend::AuthBackend;
use crate::context::AuthContext;
use crate::error::AuthError;
use crate::jwt::{extract_api_key, extract_bearer_token};
use crate::types::AuthTier;

/// Authenticated user extractor (JWT only)
#[derive(Debug)]
pub struct AuthUser(pub AuthContext);

impl<S> FromRequestParts<S> for AuthUser
where
    AuthBackend: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let backend = AuthBackend::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorization)?;

        let token = extract_bearer_token(auth_header)?;
        let auth_context = backend.authenticate_jwt(&token).await?;

        Ok(AuthUser(auth_context))
    }
}

/// Creator-tier authenticated user extractor.
///
/// Like `AuthUser` but rejects non-creator users with 403 FORBIDDEN.
/// Use this for endpoints restricted to Creator tier per spec 9.1
/// (all `/v1/teams/*` routes).
#[derive(Debug)]
pub struct CreatorUser(pub AuthContext);

impl<S> FromRequestParts<S> for CreatorUser
where
    AuthBackend: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let AuthUser(auth_context) = AuthUser::from_request_parts(parts, state).await?;

        if auth_context.user.tier != AuthTier::Creator {
            return Err(AuthError::InsufficientTier);
        }

        Ok(CreatorUser(auth_context))
    }
}

/// API Key authenticated user extractor
#[derive(Debug)]
pub struct ApiKeyUser(pub AuthContext);

impl<S> FromRequestParts<S> for ApiKeyUser
where
    AuthBackend: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let backend = AuthBackend::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorization)?;

        let api_key_str = extract_api_key(auth_header)?;
        let auth_context = backend.authenticate_api_key_full(&api_key_str).await?;

        Ok(ApiKeyUser(auth_context))
    }
}

/// Dual-auth extractor: tries API key first (if `sk_live_` prefix), falls back to JWT.
///
/// Use this for endpoints that accept **both** JWT and API key authentication.
/// The discriminator is the token format:
/// - `Bearer sk_live_...` or `sk_live_...` -> API key path
/// - `Bearer <other>` -> JWT path
#[derive(Debug)]
pub struct AnyAuth(pub AuthContext);

impl<S> FromRequestParts<S> for AnyAuth
where
    AuthBackend: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let backend = AuthBackend::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorization)?;

        let header_str = auth_header
            .to_str()
            .map_err(|_| AuthError::InvalidAuthorizationFormat)?;

        // Determine auth method by token format
        let is_api_key = if let Some(token) = header_str.strip_prefix("Bearer ") {
            token.starts_with("sk_live_")
        } else {
            header_str.starts_with("sk_live_")
        };

        if is_api_key {
            let api_key_str = extract_api_key(auth_header)?;
            let auth_context = backend.authenticate_api_key_full(&api_key_str).await?;
            Ok(AnyAuth(auth_context))
        } else {
            let token = extract_bearer_token(auth_header)?;
            let auth_context = backend.authenticate_jwt(&token).await?;
            Ok(AnyAuth(auth_context))
        }
    }
}
