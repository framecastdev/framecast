//! Custom axum extractors for Framecast

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::Error;

/// JSON extractor that validates the deserialized value automatically.
///
/// Replaces `Json<T>` + manual `.validate()` calls in handlers.
/// Requires `T: DeserializeOwned + Validate`.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) =
            Json::<T>::from_request(req, state)
                .await
                .map_err(|e: JsonRejection| {
                    Error::Validation(format!("Invalid request body: {}", e))
                })?;
        value
            .validate()
            .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;
        Ok(ValidatedJson(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{self, Request as HttpRequest};
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
        let msg = err.to_string();
        assert!(
            msg.contains("Invalid request body"),
            "Expected 'Invalid request body' in: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_validated_json_validation_failure() {
        // Empty name violates min=1 constraint
        let req = json_request(r#"{"name": ""}"#);
        let result = ValidatedJson::<TestPayload>::from_request(req, &()).await;
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Validation failed"),
            "Expected 'Validation failed' in: {}",
            msg
        );
    }
}
