//! Custom axum extractors for Framecast

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::Error;

/// JSON extractor that validates the deserialized value automatically.
///
/// Replaces `Json<T>` + manual `.validate()` calls in handlers.
/// Requires `T: DeserializeOwned + Validate`.
///
/// Deserialization failures preserve axum's native `JsonRejection` (422).
/// Validation failures return `Error::Validation` (400).
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

/// Rejection type for `ValidatedJson` that preserves the original status codes:
/// - JSON deserialization errors → 422 (from axum's `JsonRejection`)
/// - Validation errors → 400 (from `Error::Validation`)
#[derive(Debug)]
pub enum ValidatedJsonRejection {
    Json(JsonRejection),
    Validation(Error),
}

impl IntoResponse for ValidatedJsonRejection {
    fn into_response(self) -> Response {
        match self {
            ValidatedJsonRejection::Json(e) => e.into_response(),
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
        // Malformed JSON triggers axum's JsonSyntaxError → 400
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_validated_json_wrong_type() {
        // Valid JSON but wrong structure triggers axum's JsonDataError → 422
        let req = json_request(r#"{"name": 123}"#);
        let result = ValidatedJson::<TestPayload>::from_request(req, &()).await;
        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
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
}
