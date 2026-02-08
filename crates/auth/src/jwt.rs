//! JWT validation and token extraction helpers

use axum::http::HeaderValue;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

use crate::claims::SupabaseClaims;
use crate::config::AuthConfig;
use crate::error::AuthError;

/// Validate JWT token from Supabase
pub(crate) fn validate_jwt_token(
    token: &str,
    config: &AuthConfig,
) -> Result<SupabaseClaims, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);

    if let Some(aud) = &config.audience {
        validation.set_audience(&[aud]);
    } else {
        validation.validate_aud = false;
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

/// Extract bearer token from Authorization header
pub(crate) fn extract_bearer_token(header: &HeaderValue) -> Result<String, AuthError> {
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
pub(crate) fn extract_api_key(header: &HeaderValue) -> Result<String, AuthError> {
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

    #[test]
    fn test_jwt_roundtrip_no_issuer_no_audience() {
        // Simulate the E2E test scenario: no issuer, no audience configured
        let config = AuthConfig {
            jwt_secret: "test-e2e-secret-key".to_string(),
            issuer: None,
            audience: None,
        };

        // Create a token matching what PyJWT generates
        let test_user_id = uuid::Uuid::new_v4().to_string();
        let claims = SupabaseClaims {
            sub: test_user_id.clone(),
            email: Some("test@test.com".to_string()),
            aud: "authenticated".to_string(),
            role: "authenticated".to_string(),
            iat: chrono::Utc::now().timestamp() as u64,
            exp: (chrono::Utc::now().timestamp() + 3600) as u64,
        };

        let header = jsonwebtoken::Header::new(Algorithm::HS256);
        let encoding_key = jsonwebtoken::EncodingKey::from_secret(config.jwt_secret.as_ref());
        let token =
            jsonwebtoken::encode(&header, &claims, &encoding_key).expect("Failed to encode JWT");

        // Validate with same config (no issuer, no audience)
        let result = validate_jwt_token(&token, &config);
        assert!(result.is_ok(), "JWT validation failed: {:?}", result.err());

        let decoded = result.unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.email, claims.email);
        assert_eq!(decoded.aud, "authenticated");
        assert_eq!(decoded.role, "authenticated");
    }
}
