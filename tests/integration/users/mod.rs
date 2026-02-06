//! User management endpoint integration tests
//!
//! Tests the 3 user management endpoints:
//! - GET /v1/account - Get current user profile
//! - PATCH /v1/account - Update user profile
//! - POST /v1/account/upgrade - Upgrade user tier

use axum::{
    body::Body,
    http::{Request, Method, StatusCode},
    Router,
};
use tower::ServiceExt;
use serde_json::{json, Value};
use uuid::Uuid;

use framecast_api::routes;
use framecast_domain::entities::UserTier;

use crate::common::{TestApp, UserFixture, assertions};

/// Create test router with all routes
async fn create_test_router(app: &TestApp) -> Router {
    routes::create_routes().with_state(app.state.clone())
}

mod test_get_profile {
    use super::*;

    #[tokio::test]
    async fn test_get_profile_with_valid_auth() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        // Verify response contains expected fields
        assert_eq!(profile["id"], user_fixture.user.id.to_string());
        assert_eq!(profile["email"], user_fixture.user.email);
        assert_eq!(profile["tier"], "starter");
        assert_eq!(profile["credits"], user_fixture.user.credits);

        // Verify sensitive fields are not exposed
        assert!(profile.get("password").is_none());
        assert!(profile.get("password_hash").is_none());

        // Verify timestamps are present and valid
        assert!(profile.get("created_at").is_some());
        assert!(profile.get("updated_at").is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_profile_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error.get("error").is_some());
        let error_obj = error.get("error").unwrap();
        assert_eq!(error_obj["code"], "MISSING_AUTHORIZATION");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_profile_with_invalid_token() {
        let app = TestApp::new().await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header("authorization", "Bearer invalid.jwt.token")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(error["error"]["code"], "INVALID_TOKEN");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_profile_creator_user() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["tier"], "creator");
        assert!(profile.get("upgraded_at").is_some());

        // Creator users should have upgraded_at timestamp (INV-U1)
        let upgraded_at = profile["upgraded_at"].as_str().unwrap();
        assert!(!upgraded_at.is_empty());

        app.cleanup().await.unwrap();
    }
}

mod test_update_profile {
    use super::*;

    #[tokio::test]
    async fn test_update_profile_valid_data() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let update_data = json!({
            "name": "Updated Name",
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"], "Updated Name");
        assert_eq!(profile["avatar_url"], "https://example.com/avatar.jpg");

        // Verify updated_at timestamp was updated (INV-TIME1)
        let original_updated_at = chrono::DateTime::parse_from_rfc3339(
            user_fixture.user.updated_at.to_rfc3339().as_str()
        ).unwrap();
        let new_updated_at = chrono::DateTime::parse_from_rfc3339(
            profile["updated_at"].as_str().unwrap()
        ).unwrap();

        assert!(new_updated_at >= original_updated_at);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_validation_errors() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Test empty name
        let invalid_data = json!({
            "name": "",
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invalid_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error.get("error").is_some());
        assert!(error["error"]["message"].as_str().unwrap().contains("validation"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_invalid_url() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let invalid_data = json!({
            "name": "Valid Name",
            "avatar_url": "not-a-valid-url"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invalid_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_name_too_long() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let invalid_data = json!({
            "name": "x".repeat(101), // Too long
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invalid_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_malformed_json() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from("{ invalid json }"))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_partial_update() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Update only name, leave avatar_url unchanged
        let update_data = json!({
            "name": "Only Name Updated"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"], "Only Name Updated");
        // avatar_url should remain unchanged
        assert_eq!(profile["avatar_url"], user_fixture.user.avatar_url);

        app.cleanup().await.unwrap();
    }
}

mod test_upgrade_tier {
    use super::*;

    #[tokio::test]
    async fn test_upgrade_starter_to_creator() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        assert_eq!(user_fixture.user.tier, UserTier::Starter);
        assert!(user_fixture.user.upgraded_at.is_none());

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["tier"], "creator");

        // Verify INV-U1: Creator users have upgrade timestamp
        assert!(profile.get("upgraded_at").is_some());
        let upgraded_at = profile["upgraded_at"].as_str().unwrap();
        assert!(!upgraded_at.is_empty());

        // Verify timestamp is recent
        let upgraded_timestamp = chrono::DateTime::parse_from_rfc3339(upgraded_at).unwrap();
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(upgraded_timestamp.with_timezone(&chrono::Utc));
        assert!(diff.num_seconds() < 60, "Upgrade timestamp should be recent");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_already_creator() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        assert_eq!(user_fixture.user.tier, UserTier::Creator);

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should return 409 Conflict for already upgraded user
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error.get("error").is_some());
        assert!(error["error"]["message"].as_str().unwrap().contains("already"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_invalid_tier() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tier": "invalid_tier"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_missing_tier_field() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_downgrade_prevention() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Attempt to downgrade from creator to starter
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tier": "starter"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should prevent downgrade
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_without_authentication() {
        let app = TestApp::new().await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("content-type", "application/json")
            .body(Body::from(json!({"tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

mod test_user_invariants_in_endpoints {
    use super::*;

    #[tokio::test]
    async fn test_credits_remain_non_negative_after_operations() {
        // Test that user operations don't violate credit invariants
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Get current profile to check credits
        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        let credits = profile["credits"].as_i64().unwrap();
        assertions::assert_credits_non_negative(credits as i32);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_tier_upgrade_sets_timestamp() {
        // Test that upgrading to creator properly sets upgraded_at (INV-U1)
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        // Verify INV-U1 compliance
        assert_eq!(profile["tier"], "creator");
        assert!(profile.get("upgraded_at").is_some());

        let upgraded_at_str = profile["upgraded_at"].as_str().unwrap();
        let upgraded_at = chrono::DateTime::parse_from_rfc3339(upgraded_at_str).unwrap();
        assertions::assert_timestamp_recent(&upgraded_at.with_timezone(&chrono::Utc));

        app.cleanup().await.unwrap();
    }
}

mod test_error_response_consistency {
    use super::*;

    #[tokio::test]
    async fn test_consistent_error_format_across_endpoints() {
        let app = TestApp::new().await.unwrap();
        let router = create_test_router(&app).await;

        // Test that all endpoints return consistent error format
        let endpoints_and_methods = vec![
            (Method::GET, "/v1/account"),
            (Method::PATCH, "/v1/account"),
            (Method::POST, "/v1/account/upgrade"),
        ];

        for (method, uri) in endpoints_and_methods {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap();

            let response = router.clone().oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let error: Value = serde_json::from_slice(&body).unwrap();

            // Verify consistent error structure
            assert!(error.get("error").is_some());
            let error_obj = error.get("error").unwrap();
            assert!(error_obj.get("code").is_some());
            assert!(error_obj.get("message").is_some());

            // Verify error codes are strings
            assert!(error_obj["code"].is_string());
            assert!(error_obj["message"].is_string());
        }

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_validation_error_details() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Test validation error includes helpful details
        let invalid_data = json!({
            "name": "", // Invalid empty name
            "avatar_url": "not-a-url" // Invalid URL
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invalid_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        // Verify error contains validation details
        assert!(error["error"]["message"].as_str().unwrap().contains("validation") ||
                error["error"]["message"].as_str().unwrap().contains("invalid"));

        app.cleanup().await.unwrap();
    }
}

mod test_profile_edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_update_profile_unicode_name() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let update_data = json!({
            "name": "José Hernández-López 日本語"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"], "José Hernández-López 日本語");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_name_at_max_length() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // UpdateProfileRequest validates name max 255 chars
        let long_name = "A".repeat(255);
        let update_data = json!({ "name": long_name });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"].as_str().unwrap().len(), 255);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_null_avatar_clears() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // First set an avatar
        let set_avatar = json!({
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(set_avatar.to_string()))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Now clear avatar by sending null
        let clear_avatar = json!({
            "avatar_url": null
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(clear_avatar.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        // Avatar should be null/empty after clearing
        assert!(
            profile["avatar_url"].is_null() || profile["avatar_url"] == "",
            "Avatar should be cleared, got: {:?}",
            profile["avatar_url"]
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_empty_body_noop() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Get original profile
        let get_request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(get_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let original_profile: Value = serde_json::from_slice(&body).unwrap();

        // Send empty update
        let empty_update = json!({});

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(empty_update.to_string()))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let updated_profile: Value = serde_json::from_slice(&body).unwrap();

        // Name should be unchanged
        assert_eq!(original_profile["name"], updated_profile["name"]);
        assert_eq!(original_profile["email"], updated_profile["email"]);

        app.cleanup().await.unwrap();
    }
}
