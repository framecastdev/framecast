//! Authentication and authorization integration tests
//!
//! Tests JWT authentication, user tier permissions, and role-based access control
//! according to the permission matrix in docs/spec/08_Permissions.md

use framecast_auth::{AuthError, AuthTier, AuthUser};
use framecast_teams::UserTier;
use serde_json::Value;

use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, Request, StatusCode},
};
use uuid::Uuid;

use crate::common::{TestApp, UserFixture};

/// Create `Parts` from an HTTP request with optional authorization header.
fn make_parts(auth_header: Option<&str>) -> axum::http::request::Parts {
    let mut builder = Request::builder();
    if let Some(value) = auth_header {
        builder = builder.header(AUTHORIZATION, value);
    }
    let (parts, _) = builder.body(()).unwrap().into_parts();
    parts
}

mod test_jwt_validation {
    use super::*;

    #[tokio::test]
    async fn test_valid_jwt_authentication() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();

        let mut parts = make_parts(Some(&format!("Bearer {}", user_fixture.jwt_token)));

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(
            auth_result.is_ok(),
            "Valid JWT should authenticate successfully"
        );

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user.id, user_fixture.user.id);
        assert_eq!(auth_context.user.tier, AuthTier::Starter);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_missing_authorization_header() {
        let app = TestApp::new().await.unwrap();

        let mut parts = make_parts(None);

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_err());

        if let Err(AuthError::MissingAuthorization) = auth_result {
            // Expected error
        } else {
            panic!("Expected MissingAuthorization error");
        }

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invalid_authorization_format() {
        let app = TestApp::new().await.unwrap();

        let mut parts = make_parts(Some("InvalidFormat token123"));

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_err());

        if let Err(AuthError::InvalidAuthorizationFormat) = auth_result {
            // Expected error
        } else {
            panic!("Expected InvalidAuthorizationFormat error");
        }

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invalid_jwt_token() {
        let app = TestApp::new().await.unwrap();

        let mut parts = make_parts(Some("Bearer invalid.jwt.token"));

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_err());

        if let Err(AuthError::InvalidToken) = auth_result {
            // Expected error
        } else {
            panic!("Expected InvalidToken error");
        }

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_expired_jwt_token() {
        let app = TestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        use jsonwebtoken::{Algorithm, EncodingKey, Header};
        use serde::Serialize;

        #[derive(Debug, Serialize)]
        struct ExpiredClaims {
            sub: String,
            email: String,
            aud: String,
            role: String,
            iat: u64,
            exp: u64,
        }

        let past_time = (chrono::Utc::now().timestamp() - 3600) as u64;

        let claims = ExpiredClaims {
            sub: user.id.to_string(),
            email: user.email.clone(),
            aud: "authenticated".to_string(),
            role: "authenticated".to_string(),
            iat: past_time,
            exp: past_time + 1,
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(app.config.jwt_secret.as_ref());
        let expired_token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

        let mut parts = make_parts(Some(&format!("Bearer {}", expired_token)));

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_err());

        if let Err(AuthError::InvalidToken) = auth_result {
            // Expected error
        } else {
            panic!("Expected InvalidToken error for expired token");
        }

        app.cleanup().await.unwrap();
    }
}

mod test_user_tier_permissions {
    use super::*;

    #[tokio::test]
    async fn test_starter_user_authentication() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();

        let mut parts = make_parts(Some(&format!("Bearer {}", user_fixture.jwt_token)));

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_ok());

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user.tier, AuthTier::Starter);

        // Starter users should have no team memberships (INV-U3)
        assert!(auth_context.memberships.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_creator_user_authentication() {
        let app = TestApp::new().await.unwrap();
        let (user_fixture, _team, _membership) =
            UserFixture::creator_with_team(&app).await.unwrap();

        let mut parts = make_parts(Some(&format!("Bearer {}", user_fixture.jwt_token)));

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_ok());

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user.tier, AuthTier::Creator);

        // Creator users can have team memberships
        assert!(!auth_context.memberships.is_empty());

        app.cleanup().await.unwrap();
    }

    /// JIT provisioning: valid JWT for unknown user auto-creates a starter account.
    #[tokio::test]
    async fn test_user_not_found_in_database() {
        let app = TestApp::new().await.unwrap();

        let fake_user_id = Uuid::new_v4();
        let fake_token = {
            use jsonwebtoken::{Algorithm, EncodingKey, Header};
            use serde::Serialize;

            #[derive(Debug, Serialize)]
            struct FakeClaims {
                sub: String,
                email: String,
                aud: String,
                role: String,
                iat: u64,
                exp: u64,
            }

            let now = chrono::Utc::now().timestamp() as u64;

            let claims = FakeClaims {
                sub: fake_user_id.to_string(),
                email: "fake@example.com".to_string(),
                aud: "authenticated".to_string(),
                role: "authenticated".to_string(),
                iat: now,
                exp: now + 3600,
            };

            let header = Header::new(Algorithm::HS256);
            let encoding_key = EncodingKey::from_secret(app.config.jwt_secret.as_ref());
            jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap()
        };

        let mut parts = make_parts(Some(&format!("Bearer {}", fake_token)));

        // JIT provisioning: unknown user is auto-created as starter
        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(
            auth_result.is_ok(),
            "JIT provisioning should auto-create user"
        );

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user.id, fake_user_id);
        assert_eq!(auth_context.user.tier, AuthTier::Starter);

        app.cleanup().await.unwrap();
    }
}

mod test_permission_matrix {
    use super::*;
    use crate::common::create_test_jwt;

    /// Test permission scenarios from docs/spec/08_Permissions.md
    #[tokio::test]
    async fn test_user_management_permissions() {
        let app = TestApp::new().await.unwrap();

        let starter_fixture = UserFixture::starter(&app).await.unwrap();
        let creator_fixture = UserFixture::creator(&app).await.unwrap();

        // Both tiers should be able to authenticate for user management
        for (user_type, fixture) in [("starter", &starter_fixture), ("creator", &creator_fixture)] {
            let mut parts = make_parts(Some(&format!("Bearer {}", fixture.jwt_token)));

            let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
            assert!(
                auth_result.is_ok(),
                "{} user should be able to authenticate for user management",
                user_type
            );

            let AuthUser(auth_context) = auth_result.unwrap();

            match auth_context.user.tier {
                AuthTier::Starter => {
                    assert!(
                        auth_context.memberships.is_empty(),
                        "Starter users cannot have team memberships"
                    );
                }
                AuthTier::Creator => {
                    // Creator users can have memberships
                }
            }
        }

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_team_creation_permissions() {
        let app = TestApp::new().await.unwrap();

        let starter_fixture = UserFixture::starter(&app).await.unwrap();
        let creator_fixture = UserFixture::creator(&app).await.unwrap();

        let mut starter_parts = make_parts(Some(&format!("Bearer {}", starter_fixture.jwt_token)));
        let starter_auth = AuthUser::from_request_parts(&mut starter_parts, &app.state)
            .await
            .unwrap();
        assert_eq!(starter_auth.0.user.tier, AuthTier::Starter);
        assert!(!starter_auth.0.is_creator());

        let mut creator_parts = make_parts(Some(&format!("Bearer {}", creator_fixture.jwt_token)));
        let creator_auth = AuthUser::from_request_parts(&mut creator_parts, &app.state)
            .await
            .unwrap();
        assert_eq!(creator_auth.0.user.tier, AuthTier::Creator);
        assert!(creator_auth.0.is_creator());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_team_membership_role_permissions() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _membership) =
            UserFixture::creator_with_team(&app).await.unwrap();

        let member_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let member_jwt = create_test_jwt(&member_user, &app.config.jwt_secret).unwrap();

        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        // Test owner permissions
        let mut owner_parts = make_parts(Some(&format!("Bearer {}", owner_fixture.jwt_token)));
        let owner_auth = AuthUser::from_request_parts(&mut owner_parts, &app.state)
            .await
            .unwrap();
        let owner_memberships = &owner_auth.0.memberships;

        assert!(!owner_memberships.is_empty());
        let owner_membership = owner_memberships
            .iter()
            .find(|m| m.team_id == team.id)
            .unwrap();
        assert_eq!(owner_membership.role, framecast_auth::AuthRole::Owner);

        // Test member permissions
        let mut member_parts = make_parts(Some(&format!("Bearer {}", member_jwt)));
        let member_auth = AuthUser::from_request_parts(&mut member_parts, &app.state)
            .await
            .unwrap();
        let member_memberships = &member_auth.0.memberships;

        assert!(!member_memberships.is_empty());
        let member_membership = member_memberships
            .iter()
            .find(|m| m.team_id == team.id)
            .unwrap();
        assert_eq!(member_membership.role, framecast_auth::AuthRole::Member);

        app.cleanup().await.unwrap();
    }
}

mod test_auth_error_responses {
    use super::*;

    #[tokio::test]
    async fn test_auth_error_status_codes() {
        use axum::response::IntoResponse;

        let missing_auth_response = AuthError::MissingAuthorization.into_response();
        assert_eq!(missing_auth_response.status(), StatusCode::UNAUTHORIZED);

        let invalid_format_response = AuthError::InvalidAuthorizationFormat.into_response();
        assert_eq!(invalid_format_response.status(), StatusCode::UNAUTHORIZED);

        let invalid_token_response = AuthError::InvalidToken.into_response();
        assert_eq!(invalid_token_response.status(), StatusCode::UNAUTHORIZED);

        let user_not_found_response = AuthError::UserNotFound.into_response();
        assert_eq!(user_not_found_response.status(), StatusCode::UNAUTHORIZED);

        let user_load_error_response = AuthError::UserLoadError.into_response();
        assert_eq!(
            user_load_error_response.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn test_auth_error_response_format() {
        use axum::response::IntoResponse;

        let response = AuthError::InvalidToken.into_response();
        let (parts, body) = response.into_parts();

        assert_eq!(parts.status, StatusCode::UNAUTHORIZED);

        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

        let json_value: serde_json::Value = serde_json::from_str(&body_str).unwrap();

        assert!(json_value.get("error").is_some());
        let error = json_value.get("error").unwrap();
        assert!(error.get("code").is_some());
        assert!(error.get("message").is_some());

        assert_eq!(
            error.get("code").unwrap().as_str().unwrap(),
            "INVALID_TOKEN"
        );
        assert_eq!(
            error.get("message").unwrap().as_str().unwrap(),
            "Invalid or expired token"
        );
    }
}

mod test_whoami {
    use super::*;
    use axum::body::Body;
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_whoami_with_jwt_auth() {
        let app = TestApp::new().await.unwrap();
        let fixture = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .uri("/v1/auth/whoami")
            .header("authorization", format!("Bearer {}", fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(result["auth_method"], "jwt");
        assert_eq!(result["user"]["id"], fixture.user.id.to_string());
        assert_eq!(result["user"]["email"], fixture.user.email);
        assert_eq!(result["user"]["tier"], "starter");
        assert!(result.get("api_key").is_none());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_whoami_with_api_key_auth() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        // Create an API key via the endpoint
        let create_req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/v1/auth/keys")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Whoami Test Key", "scopes": ["generate", "generations:read"]})
                    .to_string(),
            ))
            .unwrap();

        let create_resp = router.clone().oneshot(create_req).await.unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);

        let create_body = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let create_result: Value = serde_json::from_slice(&create_body).unwrap();
        let raw_key = create_result["raw_key"].as_str().unwrap().to_string();

        // Call whoami with the API key
        let whoami_req = Request::builder()
            .uri("/v1/auth/whoami")
            .header("authorization", format!("Bearer {}", raw_key))
            .body(Body::empty())
            .unwrap();

        let whoami_resp = router.oneshot(whoami_req).await.unwrap();
        assert_eq!(whoami_resp.status(), StatusCode::OK);

        let whoami_body = axum::body::to_bytes(whoami_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&whoami_body).unwrap();

        assert_eq!(result["auth_method"], "api_key");
        assert_eq!(result["user"]["id"], creator.user.id.to_string());
        assert_eq!(result["user"]["tier"], "creator");

        let api_key_info = &result["api_key"];
        assert!(api_key_info.get("id").is_some());
        assert_eq!(api_key_info["name"], "Whoami Test Key");
        assert!(api_key_info["key_prefix"]
            .as_str()
            .unwrap()
            .starts_with("sk_live_"));
        let scopes = api_key_info["scopes"].as_array().unwrap();
        assert!(scopes.contains(&json!("generate")));
        assert!(scopes.contains(&json!("generations:read")));
        assert!(api_key_info["owner"].as_str().unwrap().contains("user:"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_whoami_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .uri("/v1/auth/whoami")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            result["error"]["code"].as_str().unwrap(),
            "MISSING_AUTHORIZATION"
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_whoami_with_invalid_token() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .uri("/v1/auth/whoami")
            .header("authorization", "Bearer invalid.jwt.token")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["error"]["code"].as_str().unwrap(), "INVALID_TOKEN");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_whoami_starter_user() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .uri("/v1/auth/whoami")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(result["auth_method"], "jwt");
        assert_eq!(result["user"]["tier"], "starter");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_whoami_creator_user() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .uri("/v1/auth/whoami")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(result["auth_method"], "jwt");
        assert_eq!(result["user"]["tier"], "creator");

        app.cleanup().await.unwrap();
    }
}
