//! Conversation handler integration tests (CON-I01 through CON-I22)

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_common::Urn;
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

// CON-I01 through CON-I07: Create conversation tests
mod test_create_conversation {
    use super::*;

    #[tokio::test]
    async fn test_create_conversation_returns_201() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "claude-sonnet-4-5-20250929"})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_conversation_response_defaults() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test-model"})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;

        assert_eq!(body["status"], "active");
        assert_eq!(body["message_count"], 0);
        assert!(body["last_message_at"].is_null());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_conversation_with_all_fields() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({
                "model": "test-model",
                "title": "My Chat",
                "system_prompt": "You are helpful."
            })),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;

        assert_eq!(body["model"], "test-model");
        assert_eq!(body["title"], "My Chat");
        assert_eq!(body["system_prompt"], "You are helpful.");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_missing_model_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(Method::POST, "/v1/conversations", &jwt, Some(json!({})));

        let resp = app.test_router().oneshot(req).await.unwrap();
        // Missing required field -> 422 or 400
        assert!(
            resp.status() == StatusCode::UNPROCESSABLE_ENTITY
                || resp.status() == StatusCode::BAD_REQUEST
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_model_101_chars_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let model = "a".repeat(101);
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": model})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_title_201_chars_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let title = "a".repeat(201);
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test", "title": title})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_system_prompt_10001_returns_422() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let prompt = "a".repeat(10001);
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test", "system_prompt": prompt})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }
}

// CON-I08 through CON-I16: CRUD tests
mod test_conversation_crud {
    use super::*;

    #[tokio::test]
    async fn test_list_returns_only_owned() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        // A creates a conversation
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt_a,
            Some(json!({"model": "test"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // B lists -> empty
        let req = authed_request(Method::GET, "/v1/conversations", &jwt_b, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_default_excludes_archived() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create active conv
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let active_conv = parse_body(resp).await;
        let active_id = active_conv["id"].as_str().unwrap();

        // Create and archive another
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let archived_conv = parse_body(resp).await;
        let archived_id = archived_conv["id"].as_str().unwrap();

        // Archive it
        let req = authed_request(
            Method::PATCH,
            &format!("/v1/conversations/{}", archived_id),
            &jwt,
            Some(json!({"status": "archived"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // Default list -> only active
        let req = authed_request(Method::GET, "/v1/conversations", &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["id"], active_id);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_returns_full_dto() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test-model", "title": "Test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::GET,
            &format!("/v1/conversations/{}", id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = parse_body(resp).await;

        assert!(body.get("id").is_some());
        assert!(body.get("user_id").is_some());
        assert!(body.get("title").is_some());
        assert!(body.get("model").is_some());
        assert!(body.get("status").is_some());
        assert!(body.get("message_count").is_some());
        assert!(body.get("created_at").is_some());
        assert!(body.get("updated_at").is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_other_user_returns_404() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt_a,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::GET,
            &format!("/v1/conversations/{}", id),
            &jwt_b,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_title() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test", "title": "Old Title"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::PATCH,
            &format!("/v1/conversations/{}", id),
            &jwt,
            Some(json!({"title": "New Title"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = parse_body(resp).await;
        assert_eq!(body["title"], "New Title");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_archive() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::PATCH,
            &format!("/v1/conversations/{}", id),
            &jwt,
            Some(json!({"status": "archived"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        assert_eq!(body["status"], "archived");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_unarchive() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create and archive
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::PATCH,
            &format!("/v1/conversations/{}", id),
            &jwt,
            Some(json!({"status": "archived"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // Unarchive
        let req = authed_request(
            Method::PATCH,
            &format!("/v1/conversations/{}", id),
            &jwt,
            Some(json!({"status": "active"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        assert_eq!(body["status"], "active");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_returns_204() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::DELETE,
            &format!("/v1/conversations/{}", id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_other_user_returns_404() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt_a,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::DELETE,
            &format!("/v1/conversations/{}", id),
            &jwt_b,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

// CON-I17 through CON-I22: Cascade and constraint tests
mod test_conversation_constraints {
    use super::*;

    #[tokio::test]
    async fn test_delete_conversation_cascades_messages() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create conv
        let req = authed_request(
            Method::POST,
            "/v1/conversations",
            &jwt,
            Some(json!({"model": "test"})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let created = parse_body(resp).await;
        let conv_id = created["id"].as_str().unwrap();

        // Send a message
        let req = authed_request(
            Method::POST,
            &format!("/v1/conversations/{}/messages", conv_id),
            &jwt,
            Some(json!({"content": "Hello"})),
        );
        app.test_router().oneshot(req).await.unwrap();

        // Delete conversation
        let req = authed_request(
            Method::DELETE,
            &format!("/v1/conversations/{}", conv_id),
            &jwt,
            None,
        );
        app.test_router().oneshot(req).await.unwrap();

        // Verify messages are gone (direct DB query)
        let conv_uuid: Uuid = conv_id.parse().unwrap();
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM messages WHERE conversation_id = $1")
                .bind(conv_uuid)
                .fetch_one(&app.pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_conversation_nullifies_artifact() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        // Create conversation directly in DB
        let conv_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO conversations (id, user_id, model, status, message_count, created_at, updated_at) \
             VALUES ($1, $2, 'test', 'active', 0, NOW(), NOW())",
        )
        .bind(conv_id)
        .bind(user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Create artifact referencing this conversation
        let artifact_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO artifacts (id, owner, created_by, kind, status, source,
                spec, conversation_id, created_at, updated_at)
            VALUES ($1, $2, $3, 'storyboard', 'pending', 'conversation', '{}', $4, NOW(), NOW())
            "#,
        )
        .bind(artifact_id)
        .bind(Urn::user(user.id).to_string())
        .bind(user.id)
        .bind(conv_id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Delete conversation
        sqlx::query("DELETE FROM conversations WHERE id = $1")
            .bind(conv_id)
            .execute(&app.pool)
            .await
            .unwrap();

        // Artifact should still exist but conversation_id should be NULL
        let row: (Option<Uuid>,) =
            sqlx::query_as("SELECT conversation_id FROM artifacts WHERE id = $1")
                .bind(artifact_id)
                .fetch_one(&app.pool)
                .await
                .unwrap();
        assert!(row.0.is_none());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_message_sequence_unique_constraint() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        let conv_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO conversations (id, user_id, model, status, message_count, created_at, updated_at) \
             VALUES ($1, $2, 'test', 'active', 0, NOW(), NOW())",
        )
        .bind(conv_id)
        .bind(user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Insert first message with sequence 1
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, sequence, created_at) \
             VALUES ($1, $2, 'user', 'Hello', 1, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(conv_id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Second message with same sequence should fail
        let result = sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, sequence, created_at) \
             VALUES ($1, $2, 'user', 'World', 1, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(conv_id)
        .execute(&app.pool)
        .await;

        assert!(result.is_err());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_message_content_empty_check() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        let conv_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO conversations (id, user_id, model, status, message_count, created_at, updated_at) \
             VALUES ($1, $2, 'test', 'active', 0, NOW(), NOW())",
        )
        .bind(conv_id)
        .bind(user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Empty content should violate CHECK constraint
        let result = sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, sequence, created_at) \
             VALUES ($1, $2, 'user', '', 1, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(conv_id)
        .execute(&app.pool)
        .await;

        assert!(result.is_err());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_user_fk_cascade() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        let conv_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO conversations (id, user_id, model, status, message_count, created_at, updated_at) \
             VALUES ($1, $2, 'test', 'active', 0, NOW(), NOW())",
        )
        .bind(conv_id)
        .bind(user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Delete user -> conversation should cascade
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&app.pool)
            .await
            .unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM conversations WHERE id = $1")
            .bind(conv_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_message_count_non_negative_check() {
        let app = ConversationsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        let conv_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO conversations (id, user_id, model, status, message_count, created_at, updated_at) \
             VALUES ($1, $2, 'test', 'active', 0, NOW(), NOW())",
        )
        .bind(conv_id)
        .bind(user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        // Try to set message_count = -1
        let result = sqlx::query("UPDATE conversations SET message_count = -1 WHERE id = $1")
            .bind(conv_id)
            .execute(&app.pool)
            .await;

        assert!(result.is_err());

        app.cleanup().await.unwrap();
    }
}
