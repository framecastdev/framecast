//! Team membership and invitation endpoint integration tests
//!
//! Tests the membership/invitation endpoints:
//! - POST /v1/teams/{team_id}/invitations - Invite member (renamed from /invite)
//! - POST /v1/invitations/{invitation_id}/accept - Accept invitation
//! - PUT /v1/invitations/{invitation_id}/decline - Decline invitation
//! - DELETE /v1/teams/{team_id}/members/{user_id} - Remove member
//! - PATCH /v1/teams/{team_id}/members/{user_id} - Update member role
//! - GET /v1/teams/{team_id}/members - List team members
//! - POST /v1/teams/{team_id}/leave - Leave team

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_api::routes;
use framecast_domain::entities::InvitationRole;

use crate::common::{
    email_mock::{test_utils::InvitationTestScenario, MockEmailService},
    TestApp, UserFixture,
};

/// Helper to convert InvitationRole to string for SQL binding
fn invitation_role_to_str(role: &InvitationRole) -> &'static str {
    match role {
        InvitationRole::Admin => "admin",
        InvitationRole::Member => "member",
        InvitationRole::Viewer => "viewer",
    }
}

/// Create test router with all routes
async fn create_test_router(app: &TestApp) -> Router {
    routes::create_routes().with_state(app.state.clone())
}

mod test_invite_member {
    use super::*;

    #[tokio::test]
    async fn test_invite_member_success_with_email_capture() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        let invite_data = json!({
            "email": scenario.invitee_email,
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();

        // Verify invitation response
        assert!(invitation.get("id").is_some());
        assert_eq!(invitation["team_id"], scenario.team.id.to_string());
        assert_eq!(invitation["email"], scenario.invitee_email);
        assert_eq!(invitation["role"], "member");
        assert_eq!(invitation["state"], "pending");
        assert_eq!(
            invitation["invited_by"],
            scenario.inviter.user.id.to_string()
        );

        // Simulate email sending
        let invitation_id = Uuid::parse_str(invitation["id"].as_str().unwrap()).unwrap();
        scenario
            .email_service
            .send_invitation_email(
                &scenario.team.name,
                scenario.team.id,
                invitation_id,
                &scenario.invitee_email,
                scenario.inviter.user.name.as_deref().unwrap_or("Unknown"),
                "member",
            )
            .await
            .unwrap();

        // Verify email was captured
        assert!(scenario
            .email_service
            .was_invitation_sent_to(&scenario.invitee_email));

        let captured_invitation_id = scenario
            .email_service
            .get_invitation_id_for_email(&scenario.invitee_email);
        assert_eq!(captured_invitation_id, Some(invitation_id));

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_member_owner_role_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (inviter_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let invite_data = json!({
            "email": "new_owner@example.com",
            "role": "owner"  // Cannot invite to owner role
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", inviter_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Cannot invite"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_member_permission_check() {
        let app = TestApp::new().await.unwrap();

        // Create team owner
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create another creator user (non-member)
        let non_member_fixture = UserFixture::creator(&app).await.unwrap();

        let router = create_test_router(&app).await;

        // Test: Non-member cannot invite
        let invite_data = json!({
            "email": "someone@example.com",
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", non_member_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_existing_member() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let existing_member = UserFixture::creator(&app).await.unwrap();

        // Add existing_member to team
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(existing_member.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        let invite_data = json!({
            "email": existing_member.user.email,
            "role": "admin"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already a member"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_pending_invitation_exists() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let invitee_email = "pending@example.com";

        // Create first invitation
        let invite_data = json!({
            "email": invitee_email,
            "role": "member"
        });

        let request1 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Try to create second invitation (should fail)
        let request2 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response2 = router.oneshot(request2).await.unwrap();

        assert_eq!(response2.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response2.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("pending invitation"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_max_pending_limit() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create 50 pending invitations (the limit per CARD-4)
        for i in 0..50 {
            let email = format!("user{}@example.com", i);
            let invite_data = json!({
                "email": email,
                "role": "member"
            });

            let request = Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/teams/{}/invitations", team.id))
                .header(
                    "authorization",
                    format!("Bearer {}", owner_fixture.jwt_token),
                )
                .header("content-type", "application/json")
                .body(Body::from(invite_data.to_string()))
                .unwrap();

            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // 51st invitation should fail
        let invite_data = json!({
            "email": "user51@example.com",
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("maximum pending invitations"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_member_cannot_invite() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create member user
        let member_fixture = UserFixture::creator(&app).await.unwrap();

        // Add as member to team
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

        let invite_data = json!({
            "email": "new_member@example.com",
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_admin_can_invite_member_but_not_owner() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create admin user
        let admin_fixture = UserFixture::creator(&app).await.unwrap();

        // Add as admin to team
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(admin_fixture.user.id)
        .bind("admin")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        // Test: Admin can invite member
        let invite_member_data = json!({
            "email": "new_member@example.com",
            "role": "member"
        });

        let request1 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", admin_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_member_data.to_string()))
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Test: Admin cannot invite owner
        let invite_owner_data = json!({
            "email": "new_owner@example.com",
            "role": "owner"
        });

        let request2 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", admin_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_owner_data.to_string()))
            .unwrap();

        let response2 = router.oneshot(request2).await.unwrap();

        assert_eq!(response2.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }
}

mod test_accept_invitation {
    use super::*;

    #[tokio::test]
    async fn test_accept_invitation_complete_workflow() {
        let scenario = InvitationTestScenario::new().await.unwrap();

        // Complete the invitation workflow
        let (invitation_id, invitee_fixture) = scenario
            .complete_invitation_workflow(InvitationRole::Member)
            .await
            .unwrap();

        let router = create_test_router(&scenario.app).await;

        // Accept the invitation
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let membership: Value = serde_json::from_slice(&body).unwrap();

        // Verify membership response
        assert!(membership.get("id").is_some());
        assert_eq!(membership["user_id"], invitee_fixture.user.id.to_string());
        assert_eq!(membership["role"], "member");

        // Verify membership exists in database
        let db_membership: (String,) =
            sqlx::query_as("SELECT role FROM memberships WHERE team_id = $1 AND user_id = $2")
                .bind(scenario.team.id)
                .bind(invitee_fixture.user.id)
                .fetch_one(&scenario.app.pool)
                .await
                .unwrap();

        assert_eq!(db_membership.0, "member");

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_invitation_wrong_email() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Send invitation
        let invitation_id = scenario
            .send_invitation(InvitationRole::Member)
            .await
            .unwrap();

        // Create different user (wrong email)
        let wrong_user = UserFixture::creator(&scenario.app).await.unwrap();

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header("authorization", format!("Bearer {}", wrong_user.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_already_accepted_invitation() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Complete invitation workflow
        let (invitation_id, invitee_fixture) = scenario
            .complete_invitation_workflow(InvitationRole::Member)
            .await
            .unwrap();

        // Accept invitation first time
        let request1 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Try to accept again
        let request2 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response2 = router.oneshot(request2).await.unwrap();

        assert_eq!(response2.status(), StatusCode::CONFLICT);

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_nonexistent_invitation() {
        let app = TestApp::new().await.unwrap();
        let user_fixture = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let fake_invitation_id = Uuid::new_v4();

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", fake_invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", user_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

mod test_decline_invitation {
    use super::*;

    #[tokio::test]
    async fn test_decline_invitation_success() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Send invitation and create invitee
        let (invitation_id, invitee_fixture) = scenario
            .complete_invitation_workflow(InvitationRole::Member)
            .await
            .unwrap();

        let request = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/invitations/{}/decline", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify no membership was created
        let membership_check: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND user_id = $2")
                .bind(scenario.team.id)
                .bind(invitee_fixture.user.id)
                .fetch_one(&scenario.app.pool)
                .await
                .unwrap();

        assert_eq!(membership_check.0, 0);

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_decline_wrong_user() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Send invitation
        let invitation_id = scenario
            .send_invitation(InvitationRole::Member)
            .await
            .unwrap();

        // Create different user
        let wrong_user = UserFixture::creator(&scenario.app).await.unwrap();

        let request = Request::builder()
            .method(Method::PUT)
            .uri(format!("/v1/invitations/{}/decline", invitation_id))
            .header("authorization", format!("Bearer {}", wrong_user.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        scenario.cleanup().await.unwrap();
    }
}

mod test_remove_member {
    use super::*;

    #[tokio::test]
    async fn test_remove_member_success() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a member to remove
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
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, member_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify member was removed
        let membership_check: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND user_id = $2")
                .bind(team.id)
                .bind(member_fixture.user.id)
                .fetch_one(&app.pool)
                .await
                .unwrap();

        assert_eq!(membership_check.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_last_owner_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Try to remove the only owner (should fail per INV-T2)
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, owner_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("last owner"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_member_cannot_remove_others() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add member
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

        // Add another member
        let other_member_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(other_member_fixture.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        // Member tries to remove other member
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, other_member_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }
}

mod test_update_member_role {
    use super::*;

    #[tokio::test]
    async fn test_update_member_role_success() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add member to promote
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

        let role_update = json!({
            "role": "admin"
        });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, member_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let membership: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(membership["role"], "admin");

        // Verify in database
        let db_membership: (String,) =
            sqlx::query_as("SELECT role FROM memberships WHERE team_id = $1 AND user_id = $2")
                .bind(team.id)
                .bind(member_fixture.user.id)
                .fetch_one(&app.pool)
                .await
                .unwrap();

        assert_eq!(db_membership.0, "admin");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_admin_cannot_promote_to_owner() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add admin
        let admin_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(admin_fixture.user.id)
        .bind("admin")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        // Add member
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

        let role_update = json!({
            "role": "owner"
        });

        // Admin tries to promote member to owner
        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, member_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", admin_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_demote_last_owner_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let role_update = json!({
            "role": "admin"
        });

        // Try to demote the only owner
        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, owner_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }
}

mod test_list_members {
    use super::*;

    #[tokio::test]
    async fn test_list_members_returns_all_team_members() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a member
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
            .uri(format!("/v1/teams/{}/members", team.id))
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
        let members: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(members.len(), 2);

        // Verify owner is in the list
        let owner_member = members
            .iter()
            .find(|m| m["user_id"] == owner_fixture.user.id.to_string())
            .expect("Owner should be in members list");
        assert_eq!(owner_member["role"], "owner");

        // Verify member is in the list
        let regular_member = members
            .iter()
            .find(|m| m["user_id"] == member_fixture.user.id.to_string())
            .expect("Member should be in members list");
        assert_eq!(regular_member["role"], "member");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_members_any_role_can_view() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a viewer
        let viewer_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(viewer_fixture.user.id)
        .bind("viewer")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        // Viewer can list members
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/members", team.id))
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
        let members: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(members.len(), 2);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_members_non_member_forbidden() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let non_member = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/members", team.id))
            .header("authorization", format!("Bearer {}", non_member.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_members_without_auth() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/members", team.id))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

mod test_leave_team {
    use super::*;

    #[tokio::test]
    async fn test_member_can_leave_team() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a member
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
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify member was removed
        let membership_check: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND user_id = $2")
                .bind(team.id)
                .bind(member_fixture.user.id)
                .fetch_one(&app.pool)
                .await
                .unwrap();

        assert_eq!(membership_check.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_viewer_can_leave_team() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a viewer
        let viewer_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(viewer_fixture.user.id)
        .bind("viewer")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header(
                "authorization",
                format!("Bearer {}", viewer_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_last_owner_cannot_leave() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Try to leave as the only owner (should fail per INV-T2)
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("last owner"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_owner_can_leave_if_other_owners_exist() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add another owner
        let other_owner = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(other_owner.user.id)
        .bind("owner")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_non_member_cannot_leave() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let non_member = UserFixture::creator(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header("authorization", format!("Bearer {}", non_member.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

mod test_invitation_lifecycle_with_email {
    use super::*;

    #[tokio::test]
    async fn test_complete_invitation_lifecycle_with_email_capture() {
        let email_service = MockEmailService::new();
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Step 1: Send invitation
        let invite_data = json!({
            "email": scenario.invitee_email,
            "role": "admin"
        });

        let invite_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let invite_response = router.clone().oneshot(invite_request).await.unwrap();
        assert_eq!(invite_response.status(), StatusCode::OK);

        let invite_body = axum::body::to_bytes(invite_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&invite_body).unwrap();
        let invitation_id = Uuid::parse_str(invitation["id"].as_str().unwrap()).unwrap();

        // Step 2: Simulate email sending
        email_service
            .send_invitation_email(
                &scenario.team.name,
                scenario.team.id,
                invitation_id,
                &scenario.invitee_email,
                scenario.inviter.user.name.as_deref().unwrap_or("Unknown"),
                "admin",
            )
            .await
            .unwrap();

        // Verify email capture
        assert!(email_service.was_invitation_sent_to(&scenario.invitee_email));
        let captured_email = email_service
            .get_latest_invitation_email(&scenario.invitee_email)
            .unwrap();
        assert!(captured_email.body_text.contains(&scenario.team.name));
        assert!(captured_email.body_text.contains("admin"));

        // Step 3: Create invitee user
        let invitee_fixture = scenario.create_invitee_user().await.unwrap();

        // Step 4: Accept invitation
        let accept_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let accept_response = router.oneshot(accept_request).await.unwrap();
        assert_eq!(accept_response.status(), StatusCode::OK);

        let accept_body = axum::body::to_bytes(accept_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let membership: Value = serde_json::from_slice(&accept_body).unwrap();

        assert_eq!(membership["role"], "admin");
        assert_eq!(membership["user_id"], invitee_fixture.user.id.to_string());

        // Step 5: Verify membership exists
        let membership_check: (String,) =
            sqlx::query_as("SELECT role FROM memberships WHERE team_id = $1 AND user_id = $2")
                .bind(scenario.team.id)
                .bind(invitee_fixture.user.id)
                .fetch_one(&scenario.app.pool)
                .await
                .unwrap();

        assert_eq!(membership_check.0, "admin");

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invitation_expiry_workflow() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create expired invitation directly in database
        let invitation = framecast_domain::entities::Invitation {
            id: Uuid::new_v4(),
            team_id: scenario.team.id,
            email: scenario.invitee_email.clone(),
            role: InvitationRole::Member,
            invited_by: scenario.inviter.user.id,
            token: Uuid::new_v4().to_string().replace("-", ""),
            created_at: chrono::Utc::now() - chrono::Duration::days(8),
            expires_at: chrono::Utc::now() - chrono::Duration::days(1),
            accepted_at: None,
            revoked_at: None,
        };

        sqlx::query(
            r#"
            INSERT INTO invitations (id, team_id, email, role, invited_by, created_at, expires_at, token)
            VALUES ($1, $2, $3, $4::invitation_role, $5, $6, $7, $8)
            "#,
        )
        .bind(invitation.id)
        .bind(invitation.team_id)
        .bind(&invitation.email)
        .bind(invitation_role_to_str(&invitation.role))
        .bind(invitation.invited_by)
        .bind(invitation.created_at)
        .bind(invitation.expires_at)
        .bind(&invitation.token)
        .execute(&scenario.app.pool)
        .await
        .unwrap();

        let invitee_fixture = scenario.create_invitee_user().await.unwrap();

        // Try to accept expired invitation
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation.id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("expired"));

        scenario.cleanup().await.unwrap();
    }
}
