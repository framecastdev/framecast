//! Custom axum extractors for Framecast

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Deserialize};
use validator::Validate;

use crate::Error;

/// Default page size for list endpoints
const DEFAULT_LIMIT: i64 = 50;

/// Maximum page size for list endpoints
const MAX_LIMIT: i64 = 100;

/// Pagination query parameters for list endpoints
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Pagination {
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

impl Pagination {
    /// Get the offset, defaulting to 0
    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }

    /// Get the limit, defaulting to 50, capped at 100
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }
}

/// JSON extractor that validates the deserialized value automatically.
///
/// Replaces `Json<T>` + manual `.validate()` calls in handlers.
/// Requires `T: DeserializeOwned + Validate`.
///
/// All input errors (deserialization + validation) return 400.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

/// Rejection type for `ValidatedJson`:
/// - JSON deserialization errors → 400 (via `Error::Validation`)
/// - Validation errors → 400 (via `Error::Validation`)
#[derive(Debug)]
pub enum ValidatedJsonRejection {
    Json(JsonRejection),
    Validation(Error),
}

impl IntoResponse for ValidatedJsonRejection {
    fn into_response(self) -> Response {
        match self {
            ValidatedJsonRejection::Json(e) => Error::Validation(e.body_text()).into_response(),
            ValidatedJsonRejection::Validation(e) => e.into_response(),
        }
    }
}

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ValidatedJsonRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(ValidatedJsonRejection::Json)?;
        value.validate().map_err(|e| {
            ValidatedJsonRejection::Validation(Error::Validation(format!(
                "Validation failed: {}",
                e
            )))
        })?;
        Ok(ValidatedJson(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{self, Request as HttpRequest, StatusCode};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Validate)]
    struct TestPayload {
        #[validate(length(min = 1, max = 10))]
        name: String,
    }

    fn json_request(body: &str) -> HttpRequest<axum::body::Body> {
        HttpRequest::builder()
            .method(http::Method::POST)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn test_validated_json_valid_input() {
        let req = json_request(r#"{"name": "hello"}"#);
        let result = ValidatedJson::<TestPayload>::from_request(req, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.name, "hello");
    }

    #[tokio::test]
    async fn test_validated_json_invalid_json() {
        let req = json_request("not json");
        let result = ValidatedJson::<TestPayload>::from_request(req, &()).await;
        let err = result.unwrap_err();
        // Malformed JSON → 400
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validated_json_wrong_type() {
        // Valid JSON but wrong structure → 400
        let req = json_request(r#"{"name": 123}"#);
        let result = ValidatedJson::<TestPayload>::from_request(req, &()).await;
        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validated_json_validation_failure() {
        // Empty name violates min=1 constraint
        let req = json_request(r#"{"name": ""}"#);
        let result = ValidatedJson::<TestPayload>::from_request(req, &()).await;
        let err = result.unwrap_err();
        // Validation failures return 400
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // Pagination tests

    #[test]
    fn test_pagination_defaults() {
        let p = Pagination {
            offset: None,
            limit: None,
        };
        assert_eq!(p.offset(), 0);
        assert_eq!(p.limit(), 50);
    }

    #[test]
    fn test_pagination_custom_values() {
        let p = Pagination {
            offset: Some(20),
            limit: Some(10),
        };
        assert_eq!(p.offset(), 20);
        assert_eq!(p.limit(), 10);
    }

    #[test]
    fn test_pagination_limit_clamped_to_max() {
        let p = Pagination {
            offset: None,
            limit: Some(500),
        };
        assert_eq!(p.limit(), 100);
    }

    #[test]
    fn test_pagination_limit_clamped_to_min() {
        let p = Pagination {
            offset: None,
            limit: Some(0),
        };
        assert_eq!(p.limit(), 1);
    }

    #[test]
    fn test_pagination_negative_offset_clamped() {
        let p = Pagination {
            offset: Some(-5),
            limit: None,
        };
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn test_pagination_negative_limit_clamped() {
        let p = Pagination {
            offset: None,
            limit: Some(-10),
        };
        assert_eq!(p.limit(), 1);
    }
}
