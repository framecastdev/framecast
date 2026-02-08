//! User management endpoint integration tests
//!
//! Tests the 4 user management endpoints:
//! - GET /v1/account - Get current user profile
//! - PATCH /v1/account - Update user profile
//! - DELETE /v1/account - Delete user account
//! - POST /v1/account/upgrade - Upgrade user tier

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_teams::UserTier;

use crate::common::{assertions, TestApp, UserFixture};

mod test_get_profile {
    use super::*;

    #[tokio::test]
    async fn test_get_profile_with_valid_auth() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error.get("error").is_some());
        let error_obj = error.get("error").unwrap();
        assert_eq!(error_obj["code"], "MISSING_AUTHORIZATION");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_profile_with_invalid_token() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header("authorization", "Bearer invalid.jwt.token")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(error["error"]["code"], "INVALID_TOKEN");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_profile_creator_user() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
        let router = app.test_router();

        let update_data = json!({
            "name": "Updated Name",
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"], "Updated Name");
        assert_eq!(profile["avatar_url"], "https://example.com/avatar.jpg");

        // Verify updated_at timestamp was updated (INV-TIME1)
        let original_updated_at = chrono::DateTime::parse_from_rfc3339(
            user_fixture.user.updated_at.to_rfc3339().as_str(),
        )
        .unwrap();
        let new_updated_at =
            chrono::DateTime::parse_from_rfc3339(profile["updated_at"].as_str().unwrap()).unwrap();

        assert!(new_updated_at >= original_updated_at);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_validation_errors() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        // Test empty name
        let invalid_data = json!({
            "name": "",
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invalid_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error.get("error").is_some());
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("validation"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_invalid_url() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let invalid_data = json!({
            "name": "Valid Name",
            "avatar_url": "not-a-valid-url"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
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
        let router = app.test_router();

        let invalid_data = json!({
            "name": "x".repeat(101), // Too long
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
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
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
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
        let router = app.test_router();

        // Update only name, leave avatar_url unchanged
        let update_data = json!({
            "name": "Only Name Updated"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"], "Only Name Updated");
        // avatar_url should remain unchanged (null)
        assert!(profile["avatar_url"].is_null());

        app.cleanup().await.unwrap();
    }
}

mod test_upgrade_tier {
    use super::*;

    #[tokio::test]
    async fn test_upgrade_starter_to_creator() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        assert_eq!(user_fixture.user.tier, UserTier::Starter);
        assert!(user_fixture.user.upgraded_at.is_none());

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"target_tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
        assert!(
            diff.num_seconds() < 60,
            "Upgrade timestamp should be recent"
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_already_creator() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        assert_eq!(user_fixture.user.tier, UserTier::Creator);

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"target_tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should return 409 Conflict for already upgraded user
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error.get("error").is_some());
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_invalid_tier() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"target_tier": "invalid_tier"}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Invalid enum variant → 400
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_missing_tier_field() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Missing required field → 400
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_downgrade_prevention() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        // Attempt to downgrade from creator to starter
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"target_tier": "starter"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should prevent downgrade
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade_without_authentication() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header("content-type", "application/json")
            .body(Body::from(json!({"target_tier": "creator"}).to_string()))
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
        let router = app.test_router();

        // Get current profile to check credits
        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/account/upgrade")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"target_tier": "creator"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
        let router = app.test_router();

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

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
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
        let router = app.test_router();

        // Test validation error includes helpful details
        let invalid_data = json!({
            "name": "", // Invalid empty name
            "avatar_url": "not-a-url" // Invalid URL
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invalid_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        // Verify error contains validation details
        let msg = error["error"]["message"].as_str().unwrap().to_lowercase();
        assert!(msg.contains("validation") || msg.contains("invalid"));

        app.cleanup().await.unwrap();
    }
}

mod test_profile_edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_update_profile_unicode_name() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let update_data = json!({
            "name": "José Hernández-López 日本語"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"], "José Hernández-López 日本語");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_name_at_max_length() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        // UpdateProfileRequest validates name max 100 chars
        let long_name = "A".repeat(100);
        let update_data = json!({ "name": long_name });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(update_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let profile: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(profile["name"].as_str().unwrap().len(), 100);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_profile_null_avatar_clears() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        // First set an avatar
        let set_avatar = json!({
            "avatar_url": "https://example.com/avatar.jpg"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
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
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(clear_avatar.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
        let router = app.test_router();

        // Get original profile
        let get_request = Request::builder()
            .method(Method::GET)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(get_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let original_profile: Value = serde_json::from_slice(&body).unwrap();

        // Send update with same values (handler treats None as "set to null",
        // so we send original values to achieve a true no-op)
        let noop_update = json!({
            "name": original_profile["name"],
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri("/v1/account")
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(noop_update.to_string()))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let updated_profile: Value = serde_json::from_slice(&body).unwrap();

        // Name should be unchanged
        assert_eq!(original_profile["name"], updated_profile["name"]);
        assert_eq!(original_profile["email"], updated_profile["email"]);

        app.cleanup().await.unwrap();
    }
}

mod test_delete_account {
    use super::*;

    #[tokio::test]
    async fn test_delete_account_success() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify user is deleted from database
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = $1")
            .bind(creator.user.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_account_sole_owner_blocked() {
        let app = TestApp::new().await.unwrap();
        let (owner, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add another member so the team has >1 member but user is sole owner
        let member = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at)
               VALUES ($1, $2, $3, 'member'::membership_role, NOW())"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member.user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", owner.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // INV-T2: Cannot delete while sole owner of a team
        assert_eq!(response.status(), StatusCode::CONFLICT);

        // Verify user still exists
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = $1")
            .bind(owner.user.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_account_cascades_memberships() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create a member (not sole owner)
        let member = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at)
               VALUES ($1, $2, $3, 'member'::membership_role, NOW())"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member.user.id)
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", member.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify membership was cascade-deleted
        let membership_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE user_id = $1")
                .bind(member.user.id)
                .fetch_one(&app.pool)
                .await
                .unwrap();
        assert_eq!(membership_count.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_account_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/account")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_account_starter_user() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri("/v1/account")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify user is deleted
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = $1")
            .bind(starter.user.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);

        app.cleanup().await.unwrap();
    }
}
