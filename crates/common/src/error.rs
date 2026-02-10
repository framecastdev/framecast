//! Common error types and handling for Framecast

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Common result type
pub type Result<T> = std::result::Result<T, Error>;

/// Common error type for the Framecast application
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unexpected error: {0}")]
    Unexpected(#[from] anyhow::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
}

impl Error {
    /// Get the appropriate HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::Authentication(_) => StatusCode::UNAUTHORIZED,
            Error::Authorization(_) => StatusCode::FORBIDDEN,
            Error::Validation(_) => StatusCode::BAD_REQUEST,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::RateLimit(_) => StatusCode::TOO_MANY_REQUESTS,
            Error::Unexpected(_)
            | Error::Database(_)
            | Error::Serialization(_)
            | Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get the error code for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            Error::Unexpected(_) => "UNEXPECTED_ERROR",
            Error::Database(_) => "DATABASE_ERROR",
            Error::Serialization(_) => "SERIALIZATION_ERROR",
            Error::Authentication(_) => "AUTHENTICATION_ERROR",
            Error::Authorization(_) => "AUTHORIZATION_ERROR",
            Error::Validation(_) => "VALIDATION_ERROR",
            Error::NotFound(_) => "NOT_FOUND",
            Error::Conflict(_) => "CONFLICT",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::RateLimit(_) => "RATE_LIMIT_EXCEEDED",
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_code = self.error_code();

        // Log internal errors with full context
        if matches!(self.status_code(), StatusCode::INTERNAL_SERVER_ERROR) {
            tracing::error!(error = %self, "Internal server error");
        }

        let body = Json(json!({
            "error": {
                "code": error_code,
                "message": self.to_string(),
            }
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            Error::Authentication("test".to_string()).status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            Error::Authorization("test".to_string()).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            Error::Validation("test".to_string()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            Error::NotFound("test".to_string()).status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_error_conflict_status_code() {
        assert_eq!(
            Error::Conflict("test".to_string()).status_code(),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn test_error_internal_status_code() {
        assert_eq!(
            Error::Internal("test".to_string()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_rate_limit_status_code() {
        assert_eq!(
            Error::RateLimit("test".to_string()).status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            Error::Authentication("test".to_string()).error_code(),
            "AUTHENTICATION_ERROR"
        );
        assert_eq!(
            Error::Validation("test".to_string()).error_code(),
            "VALIDATION_ERROR"
        );
        assert_eq!(Error::Conflict("test".to_string()).error_code(), "CONFLICT");
        assert_eq!(
            Error::Internal("test".to_string()).error_code(),
            "INTERNAL_ERROR"
        );
        assert_eq!(
            Error::RateLimit("test".to_string()).error_code(),
            "RATE_LIMIT_EXCEEDED"
        );
    }
}
