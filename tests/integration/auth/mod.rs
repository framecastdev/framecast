//! Authentication and authorization integration tests
//!
//! Tests JWT authentication, user tier permissions, and role-based access control
//! according to the permission matrix in docs/spec/08_Permissions.md

use framecast_teams::{AuthUser, ApiKeyUser, AuthError, UserTier, MembershipRole};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, HeaderValue, request::Parts, StatusCode},
};
use uuid::Uuid;

use crate::common::{TestApp, UserFixture, create_test_jwt, assertions};

mod test_jwt_validation {
    use super::*;

    #[tokio::test]
    async fn test_valid_jwt_authentication() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::starter(&app).await.unwrap();

        // Create mock request parts with valid JWT
        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", user_fixture.jwt_token)).unwrap(),
        );

        // Test JWT extraction
        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_ok(), "Valid JWT should authenticate successfully");

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user().id, user_fixture.user.id);
        assert_eq!(auth_context.user().tier, UserTier::Starter);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_missing_authorization_header() {
        let app = TestApp::new().await.unwrap();

        let mut parts = Parts::default();
        // No authorization header

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

        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("InvalidFormat token123"),
        );

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

        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer invalid.jwt.token"),
        );

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

        // Create expired JWT (past exp time)
        use jsonwebtoken::{Algorithm, EncodingKey, Header};
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Serialize, Deserialize)]
        struct ExpiredClaims {
            sub: String,
            email: String,
            aud: String,
            role: String,
            iat: u64,
            exp: u64, // Expired
        }

        let past_time = (chrono::Utc::now().timestamp() - 3600) as u64; // 1 hour ago

        let claims = ExpiredClaims {
            sub: user.id.to_string(),
            email: user.email.clone(),
            aud: "authenticated".to_string(),
            role: "authenticated".to_string(),
            iat: past_time,
            exp: past_time + 1, // Expired 1 hour ago
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(app.config.jwt_secret.as_ref());
        let expired_token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", expired_token)).unwrap(),
        );

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

        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", user_fixture.jwt_token)).unwrap(),
        );

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_ok());

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user().tier, UserTier::Starter);

        // Starter users should have no team memberships (INV-U3)
        assert!(auth_context.memberships().is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_creator_user_authentication() {
        let app = TestApp::new().await.unwrap();
        let (user_fixture, _team, _membership) = UserFixture::creator_with_team(&app).await.unwrap();

        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", user_fixture.jwt_token)).unwrap(),
        );

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_ok());

        let AuthUser(auth_context) = auth_result.unwrap();
        assert_eq!(auth_context.user().tier, UserTier::Creator);

        // Creator users can have team memberships
        assert!(!auth_context.memberships().is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_user_not_found_in_database() {
        let app = TestApp::new().await.unwrap();

        // Create JWT for user that doesn't exist in database
        let fake_user_id = Uuid::new_v4();
        let fake_token = {
            use jsonwebtoken::{Algorithm, EncodingKey, Header};
            use serde::{Deserialize, Serialize};

            #[derive(Debug, Serialize, Deserialize)]
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

        let mut parts = Parts::default();
        parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", fake_token)).unwrap(),
        );

        let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
        assert!(auth_result.is_err());

        if let Err(AuthError::UserNotFound) = auth_result {
            // Expected error
        } else {
            panic!("Expected UserNotFound error");
        }

        app.cleanup().await.unwrap();
    }
}

mod test_permission_matrix {
    use super::*;

    /// Test permission scenarios from docs/spec/08_Permissions.md
    #[tokio::test]
    async fn test_user_management_permissions() {
        let app = TestApp::new().await.unwrap();

        // Test both starter and creator can access user management endpoints
        let starter_fixture = UserFixture::starter(&app).await.unwrap();
        let creator_fixture = UserFixture::creator(&app).await.unwrap();

        // Both tiers should be able to authenticate for user management
        for (user_type, fixture) in [("starter", &starter_fixture), ("creator", &creator_fixture)] {
            let mut parts = Parts::default();
            parts.headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", fixture.jwt_token)).unwrap(),
            );

            let auth_result = AuthUser::from_request_parts(&mut parts, &app.state).await;
            assert!(
                auth_result.is_ok(),
                "{} user should be able to authenticate for user management",
                user_type
            );

            let AuthUser(auth_context) = auth_result.unwrap();

            // Validate user tier permissions
            match auth_context.user().tier {
                UserTier::Starter => {
                    assert!(auth_context.memberships().is_empty(), "Starter users cannot have team memberships");
                }
                UserTier::Creator => {
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

        // Check that starter users cannot create teams (will be enforced in handler)
        let mut starter_parts = Parts::default();
        starter_parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", starter_fixture.jwt_token)).unwrap(),
        );

        let starter_auth = AuthUser::from_request_parts(&mut starter_parts, &app.state).await.unwrap();
        assert_eq!(starter_auth.0.user().tier, UserTier::Starter);
        assert!(!starter_auth.0.user().can_create_teams());

        // Check that creator users can create teams
        let mut creator_parts = Parts::default();
        creator_parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", creator_fixture.jwt_token)).unwrap(),
        );

        let creator_auth = AuthUser::from_request_parts(&mut creator_parts, &app.state).await.unwrap();
        assert_eq!(creator_auth.0.user().tier, UserTier::Creator);
        assert!(creator_auth.0.user().can_create_teams());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_team_membership_role_permissions() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, membership) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create another creator user who will be a member
        let member_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let member_jwt = create_test_jwt(&member_user, &app.config.jwt_secret).unwrap();

        // Add member to team with Member role (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

        // Test owner permissions
        let mut owner_parts = Parts::default();
        owner_parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", owner_fixture.jwt_token)).unwrap(),
        );

        let owner_auth = AuthUser::from_request_parts(&mut owner_parts, &app.state).await.unwrap();
        let owner_memberships = owner_auth.0.memberships();

        assert!(!owner_memberships.is_empty());
        let owner_membership = owner_memberships.iter().find(|m| m.team_id == team.id).unwrap();
        assert_eq!(owner_membership.role, MembershipRole::Owner);

        // Test member permissions
        let mut member_parts = Parts::default();
        member_parts.headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", member_jwt)).unwrap(),
        );

        let member_auth = AuthUser::from_request_parts(&mut member_parts, &app.state).await.unwrap();
        let member_memberships = member_auth.0.memberships();

        assert!(!member_memberships.is_empty());
        let member_membership = member_memberships.iter().find(|m| m.team_id == team.id).unwrap();
        assert_eq!(member_membership.role, MembershipRole::Member);

        app.cleanup().await.unwrap();
    }
}

mod test_auth_error_responses {
    use super::*;

    #[tokio::test]
    async fn test_auth_error_status_codes() {
        // Test that different auth errors return correct HTTP status codes
        use axum::response::IntoResponse;

        let missing_auth_response = AuthError::MissingAuthorization.into_response();
        assert_eq!(
            missing_auth_response.status(),
            StatusCode::UNAUTHORIZED
        );

        let invalid_format_response = AuthError::InvalidAuthorizationFormat.into_response();
        assert_eq!(
            invalid_format_response.status(),
            StatusCode::UNAUTHORIZED
        );

        let invalid_token_response = AuthError::InvalidToken.into_response();
        assert_eq!(
            invalid_token_response.status(),
            StatusCode::UNAUTHORIZED
        );

        let user_not_found_response = AuthError::UserNotFound.into_response();
        assert_eq!(
            user_not_found_response.status(),
            StatusCode::UNAUTHORIZED
        );

        let user_load_error_response = AuthError::UserLoadError.into_response();
        assert_eq!(
            user_load_error_response.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn test_auth_error_response_format() {
        use axum::response::IntoResponse;
        use axum::body::Body;
        use hyper::body::to_bytes;

        let response = AuthError::InvalidToken.into_response();
        let (parts, body) = response.into_parts();

        assert_eq!(parts.status, StatusCode::UNAUTHORIZED);

        // Convert body to bytes and parse JSON
        let body_bytes = to_bytes(body).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

        let json_value: serde_json::Value = serde_json::from_str(&body_str).unwrap();

        assert!(json_value.get("error").is_some());
        let error = json_value.get("error").unwrap();
        assert!(error.get("code").is_some());
        assert!(error.get("message").is_some());

        assert_eq!(error.get("code").unwrap().as_str().unwrap(), "INVALID_TOKEN");
        assert_eq!(error.get("message").unwrap().as_str().unwrap(), "Invalid or expired token");
    }
}
