//! Team endpoint integration tests
//!
//! Tests:
//! - GET /v1/teams - List teams for current user

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use framecast_api::routes;

use crate::common::{TestApp, UserFixture};

/// Create test router with all routes
async fn create_test_router(app: &TestApp) -> Router {
    routes::create_routes().with_state(app.state.clone())
}

mod test_list_teams {
    use super::*;

    #[tokio::test]
    async fn test_list_teams_returns_user_teams() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

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
        let creator = app
            .create_test_user(framecast_domain::entities::UserTier::Creator)
            .await
            .unwrap();
        let jwt = crate::common::create_test_jwt(&creator, &app.config.jwt_secret).unwrap();

        // Create two teams where user is owner
        let (team1, _) = app.create_test_team(creator.id).await.unwrap();
        let (team2, _) = app.create_test_team(creator.id).await.unwrap();

        let router = create_test_router(&app).await;

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
        let router = create_test_router(&app).await;

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

        let router = create_test_router(&app).await;

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
        let router = create_test_router(&app).await;

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
    async fn test_list_teams_starter_user_gets_empty() {
        let app = TestApp::new().await.unwrap();
        let starter = UserFixture::starter(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/v1/teams")
            .header("authorization", format!("Bearer {}", starter.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Starter users can list teams (they just have none)
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert!(teams.is_empty());

        app.cleanup().await.unwrap();
    }
}
