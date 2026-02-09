//! Message handler integration tests (MSG-I01 through MSG-I08)

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use framecast_teams::UserTier;

use crate::common::{create_test_jwt, ConversationsTestApp};

/// Helper: build an authenticated request
fn authed_request(method: Method, uri: &str, jwt: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", jwt));

    if let Some(b) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(Body::from(serde_json::to_string(&b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

/// Helper: parse response body as JSON Value
async fn parse_body(response: axum::http::Response<Body>) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Helper: create a conversation and return its ID
async fn create_conversation(app: &ConversationsTestApp, jwt: &str) -> String {
    let req = authed_request(
        Method::POST,
        "/v1/conversations",
        jwt,
        Some(json!({"model": "test-model"})),
    );
    let resp = app.test_router().oneshot(req).await.unwrap();
    let body = parse_body(resp).await;
    body["id"].as_str().unwrap().to_string()
}

mod test_send_message {
    use super::*;

    #[tokio::test]
    async fn test_send_creates_two_messages() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": "Hello"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = parse_body(resp).await;
        assert!(body.get("user_message").is_some());
        assert!(body.get("assistant_message").is_some());
        assert_eq!(body["user_message"]["role"], "user");
        assert_eq!(body["assistant_message"]["role"], "assistant");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_increments_count_by_two() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        // Send a message
        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": "Hello"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // Check conversation
        let req = authed_request(
            Method::GET,
            &format!("/v1/conversations/{}", conv_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        assert_eq!(body["message_count"], 2);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_updates_last_message_at() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        // Send a message
        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": "Hello"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // Check conversation
        let req = authed_request(
            Method::GET,
            &format!("/v1/conversations/{}", conv_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        assert!(!body["last_message_at"].is_null());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_to_archived_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        // Archive it
        let req = authed_request(
            Method::PATCH,
            &format!("/v1/conversations/{}", conv_id),
            &jwt,
            Some(json!({"status": "archived"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // Try to send message
        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": "Hello"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_empty_content_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": ""})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_whitespace_content_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": "   "})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }
}

mod test_list_messages {
    use super::*;

    #[tokio::test]
    async fn test_list_messages_ordered_by_sequence() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let conv_id = create_conversation(&app, &jwt).await;

        // Send two messages
        for content in ["First", "Second"] {
            let req = authed_request(
                Method::POST,
                &format!("/v1/conversations/{}/messages", conv_id),
                &jwt,
                Some(json!({"content": content})),
            );
            app.test_router().oneshot(req).await.unwrap();
        }

        // List messages
        let req = authed_request(
            Method::GET,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        // 2 sends * 2 messages each = 4 messages
        assert_eq!(body.len(), 4);

        // Verify ordering by sequence
        let sequences: Vec<i64> = body
            .iter()
            .map(|m| m["sequence"].as_i64().unwrap())
            .collect();
        assert_eq!(sequences, vec![1, 2, 3, 4]);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_other_users_messages_returns_404() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let conv_id = create_conversation(&app, &jwt_a).await;

        let req = authed_request(
            Method::GET,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt_b,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}
