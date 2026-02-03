//! Team membership and invitation endpoint integration tests
//!
//! Tests the 5 membership/invitation endpoints:
//! - POST /v1/teams/:team_id/invite - Invite member
//! - PUT /v1/invitations/:invitation_id/accept - Accept invitation
//! - PUT /v1/invitations/:invitation_id/decline - Decline invitation
//! - DELETE /v1/teams/:team_id/members/:user_id - Remove member
//! - PUT /v1/teams/:team_id/members/:user_id/role - Update member role

use axum::{
    body::Body,
    http::{Request, Method, StatusCode},
    Router,
};
use tower::ServiceExt;
use serde_json::{json, Value};
use uuid::Uuid;

use framecast_api::routes;
use framecast_domain::entities::{UserTier, MembershipRole, InvitationRole, InvitationState};

use crate::common::{TestApp, UserFixture, assertions, email_mock::{MockEmailService, test_utils::InvitationTestScenario}};

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
            .uri(&format!("/v1/teams/{}/invite", scenario.team.id))
            .header("authorization", format!("Bearer {}", scenario.inviter.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();

        // Verify invitation response
        assert!(invitation.get("id").is_some());
        assert_eq!(invitation["team_id"], scenario.team.id.to_string());
        assert_eq!(invitation["email"], scenario.invitee_email);
        assert_eq!(invitation["role"], "member");
        assert_eq!(invitation["state"], "pending");
        assert_eq!(invitation["invited_by"], scenario.inviter.user.id.to_string());

        // Simulate email sending
        let invitation_id = Uuid::parse_str(invitation["id"].as_str().unwrap()).unwrap();
        scenario.email_service
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
        assert!(scenario.email_service.was_invitation_sent_to(&scenario.invitee_email));

        let captured_invitation_id = scenario.email_service.get_invitation_id_for_email(&scenario.invitee_email);
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
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", inviter_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("Cannot invite"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_member_permission_check() {
        let app = TestApp::new().await.unwrap();

        // Create team owner
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

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
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", non_member_fixture.jwt_token))
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

        // Add existing_member to team (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        let invite_data = json!({
            "email": existing_member.user.email,
            "role": "admin"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("already a member"));

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
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Try to create second invitation (should fail)
        let request2 = Request::builder()
            .method(Method::POST)
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response2 = router.oneshot(request2).await.unwrap();

        assert_eq!(response2.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response2.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("pending invitation"));

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
                .uri(&format!("/v1/teams/{}/invite", team.id))
                .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
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
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("maximum pending invitations"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_member_cannot_invite() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create member user
        let member_fixture = UserFixture::creator(&app).await.unwrap();

        // Add as member to team (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        let invite_data = json!({
            "email": "new_member@example.com",
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", member_fixture.jwt_token))
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
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create admin user
        let admin_fixture = UserFixture::creator(&app).await.unwrap();

        // Add as admin to team (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        // Test: Admin can invite member
        let invite_member_data = json!({
            "email": "new_member@example.com",
            "role": "member"
        });

        let request1 = Request::builder()
            .method(Method::POST)
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", admin_fixture.jwt_token))
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
            .uri(&format!("/v1/teams/{}/invite", team.id))
            .header("authorization", format!("Bearer {}", admin_fixture.jwt_token))
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
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation_id))
            .header("authorization", format!("Bearer {}", invitee_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let membership: Value = serde_json::from_slice(&body).unwrap();

        // Verify membership response
        assert!(membership.get("id").is_some());
        assert_eq!(membership["user_id"], invitee_fixture.user.id.to_string());
        assert_eq!(membership["role"], "member");

        // Verify membership exists in database (using runtime query to avoid sqlx offline mode issues)
        let db_membership: (String,) = sqlx::query_as(
            "SELECT role FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(scenario.team.id)
        .bind(invitee_fixture.user.id)
        .fetch_one(&scenario.app.pool).await.unwrap();

        assert_eq!(db_membership.0, "member");

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_invitation_starter_user_forbidden() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Send invitation
        let invitation_id = scenario.send_invitation(InvitationRole::Member).await.unwrap();

        // Create starter user (cannot accept team invitations per INV-M4)
        let starter_invitee = UserFixture::starter(&scenario.app).await.unwrap();

        let request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation_id))
            .header("authorization", format!("Bearer {}", starter_invitee.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("creator tier"));

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_accept_invitation_wrong_email() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Send invitation
        let invitation_id = scenario.send_invitation(InvitationRole::Member).await.unwrap();

        // Create different user (wrong email)
        let wrong_user = UserFixture::creator(&scenario.app).await.unwrap();

        let request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation_id))
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
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation_id))
            .header("authorization", format!("Bearer {}", invitee_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Try to accept again
        let request2 = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation_id))
            .header("authorization", format!("Bearer {}", invitee_fixture.jwt_token))
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
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", fake_invitation_id))
            .header("authorization", format!("Bearer {}", user_fixture.jwt_token))
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
            .uri(&format!("/v1/invitations/{}/decline", invitation_id))
            .header("authorization", format!("Bearer {}", invitee_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify no membership was created (using runtime query to avoid sqlx offline mode issues)
        let membership_check: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(scenario.team.id)
        .bind(invitee_fixture.user.id)
        .fetch_one(&scenario.app.pool).await.unwrap();

        assert_eq!(membership_check.0, 0);

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_decline_wrong_user() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Send invitation
        let invitation_id = scenario.send_invitation(InvitationRole::Member).await.unwrap();

        // Create different user
        let wrong_user = UserFixture::creator(&scenario.app).await.unwrap();

        let request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/decline", invitation_id))
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

        // Add a member to remove (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(&format!("/v1/teams/{}/members/{}", team.id, member_fixture.user.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify member was removed (using runtime query to avoid sqlx offline mode issues)
        let membership_check: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team.id)
        .bind(member_fixture.user.id)
        .fetch_one(&app.pool).await.unwrap();

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
            .uri(&format!("/v1/teams/{}/members/{}", team.id, owner_fixture.user.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("last owner"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_member_cannot_remove_others() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add member (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        // Member tries to remove other member
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(&format!("/v1/teams/{}/members/{}", team.id, other_member_fixture.user.id))
            .header("authorization", format!("Bearer {}", member_fixture.jwt_token))
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

        // Add member to promote (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        let role_update = json!({
            "role": "admin"
        });

        let request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/teams/{}/members/{}/role", team.id, member_fixture.user.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let membership: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(membership["role"], "admin");

        // Verify in database (using runtime query to avoid sqlx offline mode issues)
        let db_membership: (String,) = sqlx::query_as(
            "SELECT role FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team.id)
        .bind(member_fixture.user.id)
        .fetch_one(&app.pool).await.unwrap();

        assert_eq!(db_membership.0, "admin");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_admin_cannot_promote_to_owner() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add admin (using runtime query to avoid sqlx offline mode issues)
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
        .execute(&app.pool).await.unwrap();

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
        .execute(&app.pool).await.unwrap();

        let router = create_test_router(&app).await;

        let role_update = json!({
            "role": "owner"
        });

        // Admin tries to promote member to owner
        let request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/teams/{}/members/{}/role", team.id, member_fixture.user.id))
            .header("authorization", format!("Bearer {}", admin_fixture.jwt_token))
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
            .method(Method::PUT)
            .uri(&format!("/v1/teams/{}/members/{}/role", team.id, owner_fixture.user.id))
            .header("authorization", format!("Bearer {}", owner_fixture.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

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
            .uri(&format!("/v1/teams/{}/invite", scenario.team.id))
            .header("authorization", format!("Bearer {}", scenario.inviter.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let invite_response = router.clone().oneshot(invite_request).await.unwrap();
        assert_eq!(invite_response.status(), StatusCode::OK);

        let invite_body = axum::body::to_bytes(invite_response.into_body(), usize::MAX).await.unwrap();
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
        let captured_email = email_service.get_latest_invitation_email(&scenario.invitee_email).unwrap();
        assert!(captured_email.body_text.contains(&scenario.team.name));
        assert!(captured_email.body_text.contains("admin"));

        // Step 3: Create invitee user
        let invitee_fixture = scenario.create_invitee_user().await.unwrap();

        // Step 4: Accept invitation
        let accept_request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation_id))
            .header("authorization", format!("Bearer {}", invitee_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let accept_response = router.oneshot(accept_request).await.unwrap();
        assert_eq!(accept_response.status(), StatusCode::OK);

        let accept_body = axum::body::to_bytes(accept_response.into_body(), usize::MAX).await.unwrap();
        let membership: Value = serde_json::from_slice(&accept_body).unwrap();

        assert_eq!(membership["role"], "admin");
        assert_eq!(membership["user_id"], invitee_fixture.user.id.to_string());

        // Step 5: Verify membership exists (using runtime query to avoid sqlx offline mode issues)
        let membership_check: (String,) = sqlx::query_as(
            "SELECT role FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(scenario.team.id)
        .bind(invitee_fixture.user.id)
        .fetch_one(&scenario.app.pool).await.unwrap();

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
            created_at: chrono::Utc::now() - chrono::Duration::days(8), // Expired
            expires_at: chrono::Utc::now() - chrono::Duration::days(1), // Expired
            accepted_at: None,
            revoked_at: None,
        };

        // Using runtime query to avoid sqlx offline mode issues
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
        .execute(&scenario.app.pool).await.unwrap();

        let invitee_fixture = scenario.create_invitee_user().await.unwrap();

        // Try to accept expired invitation
        let request = Request::builder()
            .method(Method::PUT)
            .uri(&format!("/v1/invitations/{}/accept", invitation.id))
            .header("authorization", format!("Bearer {}", invitee_fixture.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"].as_str().unwrap().contains("expired"));

        scenario.cleanup().await.unwrap();
    }
}
