//! API key endpoint integration tests
//!
//! Tests:
//! - GET /v1/auth/keys         — List user's API keys
//! - GET /v1/auth/keys/{id}    — Get single API key
//! - POST /v1/auth/keys        — Create new API key
//! - PATCH /v1/auth/keys/{id}  — Update API key name
//! - DELETE /v1/auth/keys/{id} — Revoke API key

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_teams::routes;

use crate::common::{TestApp, UserFixture};

/// Create test router with all routes
async fn create_test_router(app: &TestApp) -> Router {
    routes().with_state(app.state.clone())
}

mod test_create_api_key {
    use super::*;

    #[tokio::test]
    async fn test_create_api_key_success() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "My Test Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();

        // Response should contain both the api_key metadata and the raw_key
        assert!(result.get("api_key").is_some());
        assert!(result.get("raw_key").is_some());

        let api_key = &result["api_key"];
        assert_eq!(api_key["name"], "My Test Key");
        assert!(api_key["key_prefix"]
            .as_str()
            .unwrap()
            .starts_with("sk_live_"));
        assert!(api_key.get("id").is_some());
        assert_eq!(api_key["user_id"], creator.user.id.to_string());
        assert!(api_key["scopes"]
            .as_array()
            .unwrap()
            .contains(&json!("generate")));

        // raw_key should start with sk_live_
        let raw_key = result["raw_key"].as_str().unwrap();
        assert!(raw_key.starts_with("sk_live_"));

        // key_hash should NOT be exposed in response
        let json_str = serde_json::to_string(&result).unwrap();
        assert!(!json_str.contains("key_hash"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_default_scopes() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Omit scopes — should default to ["*"] for creator
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        let scopes = result["api_key"]["scopes"].as_array().unwrap();
        assert!(scopes.contains(&json!("*")));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_starter_default_scopes_blocked() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Starter user omits scopes — defaults to ["*"] which is NOT in STARTER_ALLOWED_SCOPES
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should be rejected because "*" scope is not allowed for starters
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_starter_allowed_scopes() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Starter user with allowed scopes should succeed
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Starter Key", "scopes": ["generate", "jobs:read"]}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_starter_restricted_scope_blocked() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // team:admin is not in STARTER_ALLOWED_SCOPES
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Bad Key", "scopes": ["team:admin"]}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_invalid_scope() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"scopes": ["nonexistent:scope"]}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_name_too_long() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "a".repeat(101)}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_api_key_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "No Auth Key"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

mod test_list_api_keys {
    use super::*;

    #[tokio::test]
    async fn test_list_api_keys_empty() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let keys: Vec<Value> = serde_json::from_slice(&body).unwrap();
        assert!(keys.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_api_keys_returns_own_keys() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create a key first
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "List Test Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::CREATED);

        // List keys
        let list_request = Request::builder()
            .method(Method::GET)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let list_response = router.oneshot(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let keys: Vec<Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0]["name"], "List Test Key");

        // key_hash should NOT be exposed
        let json_str = serde_json::to_string(&keys[0]).unwrap();
        assert!(!json_str.contains("key_hash"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_api_keys_isolation_between_users() {
        let app = TestApp::new().await.unwrap();
        let creator1 = UserFixture::creator(&app).await.unwrap();
        let creator2 = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Creator 1 creates a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator1.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "User1 Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::CREATED);

        // Creator 2 should see empty list
        let list_request = Request::builder()
            .method(Method::GET)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator2.jwt_token))
            .body(Body::empty())
            .unwrap();

        let list_response = router.oneshot(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let keys: Vec<Value> = serde_json::from_slice(&body).unwrap();
        assert!(keys.is_empty());

        app.cleanup().await.unwrap();
    }
}

mod test_get_api_key {
    use super::*;

    #[tokio::test]
    async fn test_get_api_key_success() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Get Test Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Get the key
        let get_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let get_response = router.oneshot(get_request).await.unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let key: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(key["name"], "Get Test Key");
        assert_eq!(key["id"], key_id);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_api_key_not_found() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/auth/keys/{}", Uuid::new_v4()))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_api_key_wrong_user_returns_404() {
        let app = TestApp::new().await.unwrap();
        let creator1 = UserFixture::creator(&app).await.unwrap();
        let creator2 = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Creator 1 creates a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator1.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Private Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Creator 2 tries to get it — should get 404 (not 403, to prevent info leak)
        let get_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator2.jwt_token))
            .body(Body::empty())
            .unwrap();

        let get_response = router.oneshot(get_request).await.unwrap();
        assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

mod test_update_api_key {
    use super::*;

    #[tokio::test]
    async fn test_update_api_key_name() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Original Name", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Update name
        let update_request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Updated Name"}).to_string()))
            .unwrap();

        let update_response = router.oneshot(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(update_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let updated: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(updated["name"], "Updated Name");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_api_key_empty_name_rejected() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "To Update", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Attempt empty name
        let update_request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": ""}).to_string()))
            .unwrap();

        let update_response = router.oneshot(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_api_key_wrong_user_returns_404() {
        let app = TestApp::new().await.unwrap();
        let creator1 = UserFixture::creator(&app).await.unwrap();
        let creator2 = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Creator 1 creates a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator1.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Private Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Creator 2 tries to update
        let update_request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator2.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Hijacked"}).to_string()))
            .unwrap();

        let update_response = router.oneshot(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

mod test_revoke_api_key {
    use super::*;

    #[tokio::test]
    async fn test_revoke_api_key_success() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "To Revoke", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Revoke it
        let revoke_request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let revoke_response = router.clone().oneshot(revoke_request).await.unwrap();
        assert_eq!(revoke_response.status(), StatusCode::NO_CONTENT);

        // Verify key shows as revoked when fetched
        let get_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let get_response = router.oneshot(get_request).await.unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let key: Value = serde_json::from_slice(&body).unwrap();
        assert!(key["revoked_at"].is_string(), "revoked_at should be set");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_revoke_already_revoked_key() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create and revoke a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Double Revoke", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // First revoke
        let revoke1 = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();
        let response1 = router.clone().oneshot(revoke1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::NO_CONTENT);

        // Second revoke should fail
        let revoke2 = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();
        let response2 = router.oneshot(revoke2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_revoke_api_key_wrong_user_returns_404() {
        let app = TestApp::new().await.unwrap();
        let creator1 = UserFixture::creator(&app).await.unwrap();
        let creator2 = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Creator 1 creates a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator1.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Private Key", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Creator 2 tries to revoke
        let revoke_request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator2.jwt_token))
            .body(Body::empty())
            .unwrap();

        let revoke_response = router.oneshot(revoke_request).await.unwrap();
        assert_eq!(revoke_response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_revoke_nonexistent_key() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/auth/keys/{}", Uuid::new_v4()))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_revoked_key_blocked() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create and revoke a key
        let create_request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "To Revoke", "scopes": ["generate"]}).to_string(),
            ))
            .unwrap();

        let create_response = router.clone().oneshot(create_request).await.unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: Value = serde_json::from_slice(&body).unwrap();
        let key_id = created["api_key"]["id"].as_str().unwrap();

        // Revoke
        let revoke_request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();
        router.clone().oneshot(revoke_request).await.unwrap();

        // Try to update revoked key
        let update_request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/auth/keys/{}", key_id))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "New Name"}).to_string()))
            .unwrap();

        let update_response = router.oneshot(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }
}
