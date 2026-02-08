//! Team endpoint integration tests
//!
//! Tests:
//! - GET /v1/teams - List teams for current user
//! - POST /v1/teams - Create a new team
//! - GET /v1/teams/{id} - Get team details
//! - PATCH /v1/teams/{id} - Update team
//! - DELETE /v1/teams/{id} - Delete team

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_teams::UserTier;

use crate::common::{TestApp, UserFixture};

mod test_list_teams {
    use super::*;

    #[tokio::test]
    async fn test_list_teams_returns_user_teams() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0]["id"], team.id.to_string());
        assert_eq!(teams[0]["name"], team.name);
        assert_eq!(teams[0]["slug"], team.slug);
        assert_eq!(teams[0]["user_role"], "owner");
        assert!(teams[0].get("user_urn").is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_teams_multiple_teams() {
        let app = TestApp::new().await.unwrap();
        let creator = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = crate::common::create_test_jwt(&creator, &app.config.jwt_secret).unwrap();

        // Create two teams where user is owner
        let (team1, _) = app.create_test_team(creator.id).await.unwrap();
        let (team2, _) = app.create_test_team(creator.id).await.unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(teams.len(), 2);

        let team_ids: Vec<&str> = teams.iter().map(|t| t["id"].as_str().unwrap()).collect();
        assert!(team_ids.contains(&team1.id.to_string().as_str()));
        assert!(team_ids.contains(&team2.id.to_string().as_str()));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_teams_empty_for_user_without_teams() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert!(teams.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_teams_includes_teams_where_user_is_member() {
        let app = TestApp::new().await.unwrap();

        // Create team owner
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create another user and add as member
        let member_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_fixture.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0]["id"], team.id.to_string());
        assert_eq!(teams[0]["user_role"], "member");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_teams_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_teams_starter_user_forbidden() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Spec 9.1: Starter users cannot access team operations
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }
}

mod test_create_team {
    use super::*;

    #[tokio::test]
    async fn test_create_team_success() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Test Team"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();

        assert!(team.get("id").is_some());
        assert_eq!(team["name"], "Test Team");
        assert!(team.get("slug").is_some());
        assert!(team.get("created_at").is_some());
        assert_eq!(team["user_role"], "owner");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_with_custom_slug() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "My Team", "slug": "custom-slug"}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(team["slug"], "custom-slug");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_auto_generated_slug() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Auto Slug Team"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();

        let slug = team["slug"].as_str().unwrap();
        assert!(!slug.is_empty());
        assert!(slug.starts_with("auto-slug-team"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_starter_forbidden() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Forbidden Team"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_duplicate_slug_conflict() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        // Create first team with explicit slug
        let request1 = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Team One", "slug": "dup-slug"}).to_string(),
            ))
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::CREATED);

        // Create second team with same slug
        let request2 = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Team Two", "slug": "dup-slug"}).to_string(),
            ))
            .unwrap();

        let response2 = router.oneshot(request2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_short_name_accepted() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        // Spec min=1: two-char name is valid
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "AB"}).to_string()))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Single-char name is also valid
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "X"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_invalid_slug_format() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Valid Name", "slug": "-invalid-"}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "No Auth Team"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_unicode_name() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        // Unicode name with explicit ASCII slug succeeds
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Café Studio 日本", "slug": "cafe-studio"}).to_string(),
            ))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(team["name"], "Café Studio 日本");
        assert_eq!(team["slug"], "cafe-studio");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_unicode_name_without_slug_rejected() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        // Unicode name without slug fails — auto-generated slug can't be ASCII-only
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "日本語チーム"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_empty_name() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": ""}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_creates_owner_membership() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Owner Test Team"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();
        let team_id = Uuid::parse_str(team["id"].as_str().unwrap()).unwrap();

        // Verify membership row exists with owner role
        let membership: (String,) = sqlx::query_as(
            "SELECT role::text FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team_id)
        .bind(creator.user.id)
        .fetch_one(&app.pool)
        .await
        .unwrap();

        assert_eq!(membership.0, "owner");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_sql_injection_in_name() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "'; DROP TABLE teams;--"}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        // Should succeed — name is stored safely via parameterized queries
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(team["name"], "'; DROP TABLE teams;--");

        // Verify teams table still exists
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams")
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert!(count.0 >= 1);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_html_in_name() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "<b>Bold Team</b>"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let team: Value = serde_json::from_slice(&body).unwrap();
        // HTML stored as-is in JSON API (no server-side sanitization needed for JSON API)
        assert_eq!(team["name"], "<b>Bold Team</b>");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_team_max_memberships_limit() {
        let app = TestApp::new().await.unwrap();
        let creator = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = crate::common::create_test_jwt(&creator, &app.config.jwt_secret).unwrap();

        // Insert 50 existing memberships to hit INV-T8 limit
        for i in 0..50 {
            let team_id = Uuid::new_v4();
            let slug = format!(
                "inv-t8-team-{}-{}",
                i,
                Uuid::new_v4().to_string().get(..8).unwrap()
            );
            sqlx::query(
                "INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at) VALUES ($1, $2, $3, 0, 0, '{}'::jsonb, NOW(), NOW())",
            )
            .bind(team_id)
            .bind(format!("INV-T8 Team {}", i))
            .bind(&slug)
            .execute(&app.pool)
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, 'member'::membership_role, NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(team_id)
            .bind(creator.id)
            .execute(&app.pool)
            .await
            .unwrap();
        }

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", jwt))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "One Too Many"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }
}

mod test_get_team {
    use super::*;

    #[tokio::test]
    async fn test_get_team_as_owner() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(result["id"], team.id.to_string());
        assert_eq!(result["name"], team.name);
        assert_eq!(result["user_role"], "owner");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_team_as_member() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let member_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_fixture.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["user_role"], "member");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_team_as_viewer() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let viewer_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(viewer_fixture.user.id)
        .bind("viewer")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", viewer_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["user_role"], "viewer");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_team_non_member_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let non_member = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}", team.id))
            .header("authorization", format!("Bearer {}", non_member.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_team_nonexistent_404() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}", Uuid::new_v4()))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_team_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

mod test_update_team {
    use super::*;

    #[tokio::test]
    async fn test_update_team_as_owner() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"name": "Updated Name", "settings": {"key": "value"}}).to_string(),
            ))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["name"], "Updated Name");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_as_admin() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let admin_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(admin_fixture.user.id)
        .bind("admin")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", admin_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Admin Updated"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_member_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let member_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_fixture.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Not Allowed"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_viewer_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let viewer_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(viewer_fixture.user.id)
        .bind("viewer")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", viewer_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Not Allowed"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_non_member_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let non_member = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header("authorization", format!("Bearer {}", non_member.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Not Allowed"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_partial_name_only() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let original_slug = team.slug.clone();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Only Name Changed"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["name"], "Only Name Changed");
        assert_eq!(result["slug"], original_slug);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_short_name_accepted() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = app.test_router();

        // Spec min=1: two-char name is valid
        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "AB"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["name"], "AB");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_team_without_auth() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!("/v1/teams/{}", team.id))
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "No Auth"}).to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

mod test_delete_team {
    use super::*;

    #[tokio::test]
    async fn test_delete_team_owner_sole_member() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify team is gone
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE id = $1")
            .bind(team.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_team_admin_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let admin_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(admin_fixture.user.id)
        .bind("admin")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", admin_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_team_with_other_members() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add another member
        let member = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/teams/{}", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_team_nonexistent_404() {
        let app = TestApp::new().await.unwrap();
        let creator = UserFixture::creator(&app).await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/teams/{}", Uuid::new_v4()))
            .header("authorization", format!("Bearer {}", creator.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_team_without_auth() {
        let app = TestApp::new().await.unwrap();
        let router = app.test_router();

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/teams/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}
