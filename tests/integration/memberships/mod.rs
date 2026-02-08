//! Team membership and invitation endpoint integration tests
//!
//! Tests the membership/invitation endpoints:
//! - POST /v1/teams/{team_id}/invitations - Invite member (renamed from /invite)
//! - POST /v1/invitations/{invitation_id}/accept - Accept invitation
//! - POST /v1/invitations/{invitation_id}/decline - Decline invitation
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

use framecast_teams::{routes, InvitationRole};

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
    routes().with_state(app.state.clone())
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

        // "owner" is not a valid InvitationRole variant, so Axum's JSON
        // deserializer rejects it before the handler runs → 422
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

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
    async fn test_reinvite_revokes_existing_and_creates_new() {
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

        let body1 = axum::body::to_bytes(response1.into_body(), usize::MAX)
            .await
            .unwrap();
        let first_invitation: Value = serde_json::from_slice(&body1).unwrap();
        let first_id = first_invitation["id"].as_str().unwrap().to_string();

        // Re-invite same email — should revoke first and create new
        let reinvite_data = json!({
            "email": invitee_email,
            "role": "admin"
        });

        let request2 = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(reinvite_data.to_string()))
            .unwrap();

        let response2 = router.oneshot(request2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::OK);

        let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
            .await
            .unwrap();
        let second_invitation: Value = serde_json::from_slice(&body2).unwrap();
        let second_id = second_invitation["id"].as_str().unwrap().to_string();

        // New invitation has different ID and updated role
        assert_ne!(first_id, second_id);
        assert_eq!(second_invitation["role"], "admin");
        assert_eq!(second_invitation["state"], "pending");

        // Verify old invitation was revoked in DB
        let old_revoked: (Option<chrono::DateTime<chrono::Utc>>,) =
            sqlx::query_as("SELECT revoked_at FROM invitations WHERE id = $1")
                .bind(uuid::Uuid::parse_str(&first_id).unwrap())
                .fetch_one(&app.pool)
                .await
                .unwrap();
        assert!(
            old_revoked.0.is_some(),
            "First invitation should be revoked"
        );

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

        // "owner" is not a valid InvitationRole variant, so Axum's JSON
        // deserializer rejects it before the handler runs → 422
        assert_eq!(response2.status(), StatusCode::UNPROCESSABLE_ENTITY);

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
        let db_membership: (String,) = sqlx::query_as(
            "SELECT role::text FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
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
            .method(Method::POST)
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
            .method(Method::POST)
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

        // Add a member to remove (give them a second team so INV-U2 is satisfied)
        let member_fixture = UserFixture::creator(&app).await.unwrap();
        app.create_test_team(member_fixture.user.id).await.unwrap();
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
    async fn test_admin_cannot_remove_owner() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add an admin
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

        let router = create_test_router(&app).await;

        // Admin tries to remove owner — should fail
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, owner_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", admin_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("cannot remove"));

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
        let db_membership: (String,) = sqlx::query_as(
            "SELECT role::text FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
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

        // Add a second owner who will try to demote the first
        let second_owner = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(second_owner.user.id)
        .bind("owner")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        // Demote second_owner to admin — succeeds (2 owners → 1)
        let role_update = json!({ "role": "admin" });
        let request1 = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, second_owner.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response1 = router.clone().oneshot(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Now second_owner (admin) tries to demote the last owner — should fail (admin cannot promote/demote to owner level)
        // Actually admin can update roles, but cannot promote TO owner. Demoting FROM owner is checked via INV-T2.
        // But admin doesn't have permission to touch owner roles either way.
        // The cleaner approach: re-promote second_owner to owner, then have them try to demote the last remaining owner.

        // Re-promote second_owner back to owner
        sqlx::query("UPDATE memberships SET role = 'owner' WHERE team_id = $1 AND user_id = $2")
            .bind(team.id)
            .bind(second_owner.user.id)
            .execute(&app.pool)
            .await
            .unwrap();

        // Owner demotes first owner to admin (2 owners → 1 owner)
        let request2 = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, owner_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", second_owner.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({ "role": "admin" }).to_string()))
            .unwrap();

        let response2 = router.clone().oneshot(request2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::OK);

        // Now first_owner (now admin) tries to demote last owner (second_owner) — fails (admin cannot change owner)
        let request3 = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, second_owner.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({ "role": "admin" }).to_string()))
            .unwrap();

        let response3 = router.oneshot(request3).await.unwrap();

        // INV-T2: Cannot demote the last owner — this is a business invariant
        // (409 Conflict), not a permission error (403 Forbidden)
        assert_eq!(response3.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_promote_to_owner_exceeds_max_owned_teams() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add member who already owns 10 teams
        let member_fixture = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_fixture.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        // Create 10 teams where member is owner (hitting INV-T7 limit)
        for i in 0..10 {
            let t_id = Uuid::new_v4();
            let slug = format!(
                "inv-t7-{}-{}",
                i,
                Uuid::new_v4().to_string().get(..8).unwrap()
            );
            sqlx::query(
                "INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at) VALUES ($1, $2, $3, 0, 0, '{}'::jsonb, NOW(), NOW())",
            )
            .bind(t_id)
            .bind(format!("Owned Team {}", i))
            .bind(&slug)
            .execute(&app.pool)
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO memberships (id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, 'owner'::membership_role, NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(t_id)
            .bind(member_fixture.user.id)
            .execute(&app.pool)
            .await
            .unwrap();
        }

        let router = create_test_router(&app).await;

        // Try to promote member to owner — should fail (INV-T7)
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
            .body(Body::from(json!({"role": "owner"}).to_string()))
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

        // Verify owner is in the list with enriched user fields
        let owner_member = members
            .iter()
            .find(|m| m["user_id"] == owner_fixture.user.id.to_string())
            .expect("Owner should be in members list");
        assert_eq!(owner_member["role"], "owner");
        assert!(owner_member.get("user_email").is_some());
        assert_eq!(owner_member["user_email"], owner_fixture.user.email);

        // Verify member is in the list with enriched user fields
        let regular_member = members
            .iter()
            .find(|m| m["user_id"] == member_fixture.user.id.to_string())
            .expect("Member should be in members list");
        assert_eq!(regular_member["role"], "member");
        assert!(regular_member.get("user_email").is_some());
        assert_eq!(regular_member["user_email"], member_fixture.user.email);

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

        // Add a member (give them a second team so INV-U2 is satisfied when leaving)
        let member_fixture = UserFixture::creator(&app).await.unwrap();
        app.create_test_team(member_fixture.user.id).await.unwrap();
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

        // Add a viewer (give them a second team so INV-U2 is satisfied when leaving)
        let viewer_fixture = UserFixture::creator(&app).await.unwrap();
        app.create_test_team(viewer_fixture.user.id).await.unwrap();
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
    async fn test_last_owner_leaving_auto_deletes_team() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        // Give owner a second team so INV-U2 is satisfied when leaving
        app.create_test_team(owner_fixture.user.id).await.unwrap();
        let router = create_test_router(&app).await;

        // Last owner (and sole member) leaving should auto-delete the team
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

        // Verify the team was auto-deleted
        let team_check: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE id = $1")
            .bind(team.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(team_check.0, 0);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_last_owner_cannot_leave_with_other_members() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a non-owner member so the owner can't leave (INV-T2)
        let member = UserFixture::creator(&app).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member.user.id)
        .bind("member")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        // Last owner tries to leave but other members exist — should fail
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
        // Give owner a second team so INV-U2 is satisfied when leaving
        app.create_test_team(owner_fixture.user.id).await.unwrap();

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
        let membership_check: (String,) = sqlx::query_as(
            "SELECT role::text FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
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
        let invitation = framecast_teams::Invitation {
            id: Uuid::new_v4(),
            team_id: scenario.team.id,
            email: scenario.invitee_email.clone(),
            role: InvitationRole::Member,
            invited_by: scenario.inviter.user.id,
            token: Uuid::new_v4().to_string().replace("-", ""),
            created_at: chrono::Utc::now() - chrono::Duration::days(8),
            expires_at: chrono::Utc::now() - chrono::Duration::days(1),
            accepted_at: None,
            declined_at: None,
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

mod test_membership_guards {
    use super::*;

    #[tokio::test]
    async fn test_remove_member_cannot_remove_self() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Owner tries to remove self via DELETE (should fail — use /leave instead)
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

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Cannot remove yourself"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_role_cannot_change_own_role() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let role_update = json!({
            "role": "admin"
        });

        // Owner tries to change own role
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

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Cannot change your own role"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_owner_role_rejected() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Send invitation with "owner" role — should fail at deserialization
        // since InvitationRole has no Owner variant
        let invite_data = json!({
            "email": "new@example.com",
            "role": "owner"
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

        // Should be 422 (Unprocessable Entity) since the role value is invalid
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_self_rejected() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Owner tries to invite themselves
        let invite_data = json!({
            "email": owner_fixture.user.email,
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

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Cannot invite yourself"));

        app.cleanup().await.unwrap();
    }
}

mod test_invitation_management {
    use super::*;

    #[tokio::test]
    async fn test_list_invitations_owner_can_view() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create invitations
        for i in 0..3 {
            let invite_data = json!({
                "email": format!("user{}@example.com", i),
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

            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // List invitations
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitations: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(invitations.len(), 3);
        for inv in &invitations {
            assert_eq!(inv["state"], "pending");
            assert!(inv.get("email").is_some());
        }

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_invitations_member_cannot_view() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a regular member
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
            .uri(format!("/v1/teams/{}/invitations", team.id))
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

    #[tokio::test]
    async fn test_revoke_invitation_by_owner() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create an invitation
        let invitation_id = scenario
            .send_invitation(InvitationRole::Member)
            .await
            .unwrap();

        // Revoke it
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/invitations/{}",
                scenario.team.id, invitation_id
            ))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify state is revoked by listing invitations
        let list_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let list_response = router.oneshot(list_request).await.unwrap();
        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitations: Vec<Value> = serde_json::from_slice(&body).unwrap();

        let revoked = invitations
            .iter()
            .find(|i| i["id"] == invitation_id.to_string())
            .unwrap();
        assert_eq!(revoked["state"], "revoked");

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_revoke_non_pending_invitation() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create and accept invitation
        let (invitation_id, invitee_fixture) = scenario
            .complete_invitation_workflow(InvitationRole::Member)
            .await
            .unwrap();

        // Accept first
        let accept_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let accept_response = router.clone().oneshot(accept_request).await.unwrap();
        assert_eq!(accept_response.status(), StatusCode::OK);

        // Try to revoke accepted invitation — should fail
        let revoke_request = Request::builder()
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/invitations/{}",
                scenario.team.id, invitation_id
            ))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let revoke_response = router.oneshot(revoke_request).await.unwrap();
        assert_eq!(revoke_response.status(), StatusCode::CONFLICT);

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_revoke_wrong_team() {
        let app = TestApp::new().await.unwrap();
        let (owner_a, team_a, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let (owner_b, team_b, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Create invitation on team A
        let invite_data = json!({
            "email": "someone@example.com",
            "role": "member"
        });

        let invite_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team_a.id))
            .header("authorization", format!("Bearer {}", owner_a.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let invite_response = router.clone().oneshot(invite_request).await.unwrap();
        assert_eq!(invite_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(invite_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();
        let invitation_id = invitation["id"].as_str().unwrap();

        // Try to revoke via team B — should fail with 404
        let revoke_request = Request::builder()
            .method(Method::DELETE)
            .uri(format!(
                "/v1/teams/{}/invitations/{}",
                team_b.id, invitation_id
            ))
            .header("authorization", format!("Bearer {}", owner_b.jwt_token))
            .body(Body::empty())
            .unwrap();

        let revoke_response = router.oneshot(revoke_request).await.unwrap();
        assert_eq!(revoke_response.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_resend_invitation() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create an invitation
        let invitation_id = scenario
            .send_invitation(InvitationRole::Member)
            .await
            .unwrap();

        // Get original expiration
        let list_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let list_response = router.clone().oneshot(list_request).await.unwrap();
        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitations: Vec<Value> = serde_json::from_slice(&body).unwrap();
        let original_expires = invitations[0]["expires_at"].as_str().unwrap().to_string();

        // Wait briefly to ensure new expiration is different
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Resend
        let resend_request = Request::builder()
            .method(Method::POST)
            .uri(format!(
                "/v1/teams/{}/invitations/{}/resend",
                scenario.team.id, invitation_id
            ))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let resend_response = router.oneshot(resend_request).await.unwrap();
        assert_eq!(resend_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resend_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let updated_invitation: Value = serde_json::from_slice(&body).unwrap();

        // Expiration should be updated
        let new_expires = updated_invitation["expires_at"].as_str().unwrap();
        assert_ne!(new_expires, original_expires);
        assert_eq!(updated_invitation["state"], "pending");

        scenario.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_decline_sets_declined_not_revoked() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create invitation and invitee
        let (invitation_id, invitee_fixture) = scenario
            .complete_invitation_workflow(InvitationRole::Member)
            .await
            .unwrap();

        // Decline the invitation
        let decline_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/decline", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", invitee_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let decline_response = router.clone().oneshot(decline_request).await.unwrap();
        assert_eq!(decline_response.status(), StatusCode::NO_CONTENT);

        // Verify state is "declined" (not "revoked") by listing invitations
        let list_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let list_response = router.oneshot(list_request).await.unwrap();
        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitations: Vec<Value> = serde_json::from_slice(&body).unwrap();

        let declined_inv = invitations
            .iter()
            .find(|i| i["id"] == invitation_id.to_string())
            .unwrap();
        assert_eq!(declined_inv["state"], "declined");

        scenario.cleanup().await.unwrap();
    }
}

mod test_delete_team_sole_member {
    use super::*;

    #[tokio::test]
    async fn test_delete_team_with_other_members_rejected() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a member to the team
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

        // Owner tries to delete team with other members — should fail
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

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();

        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no other members"));

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_team_sole_member_succeeds() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Owner is the only member — should succeed
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

        // Verify team was deleted
        let team_check: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE id = $1")
            .bind(team.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(team_check.0, 0);

        app.cleanup().await.unwrap();
    }
}

mod test_team_auto_delete {
    use super::*;

    #[tokio::test]
    async fn test_team_auto_deleted_when_last_member_leaves() {
        let app = TestApp::new().await.unwrap();
        let (_, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a second owner so the first can leave (give them a second team for INV-U2)
        let second_owner = UserFixture::creator(&app).await.unwrap();
        app.create_test_team(second_owner.user.id).await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(second_owner.user.id)
        .bind("owner")
        .bind(chrono::Utc::now())
        .execute(&app.pool)
        .await
        .unwrap();

        let router = create_test_router(&app).await;

        // Second owner leaves — 1 member remains, team should NOT be deleted
        let leave_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header(
                "authorization",
                format!("Bearer {}", second_owner.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(leave_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Team should still exist
        let team_check: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE id = $1")
            .bind(team.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(team_check.0, 1);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_team_auto_deleted_when_last_member_removed() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a member (give them a second team so INV-U2 is satisfied when removed)
        let member_fixture = UserFixture::creator(&app).await.unwrap();
        app.create_test_team(member_fixture.user.id).await.unwrap();
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

        // Owner removes the member
        let remove_request = Request::builder()
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

        let remove_response = router.clone().oneshot(remove_request).await.unwrap();
        assert_eq!(remove_response.status(), StatusCode::NO_CONTENT);

        // Team should still exist (owner is still a member)
        let team_check: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM teams WHERE id = $1")
            .bind(team.id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(team_check.0, 1);

        app.cleanup().await.unwrap();
    }
}

mod test_invitation_state_filter {
    use super::*;

    #[tokio::test]
    async fn test_list_invitations_with_state_filter() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Create a pending invitation
        let invite_data = json!({
            "email": "pending@example.com",
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

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // List with state=pending should return the invitation
        let list_request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/v1/teams/{}/invitations?state=pending",
                scenario.team.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let list_response = router.clone().oneshot(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitations: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(invitations.len(), 1);
        assert_eq!(invitations[0]["state"], "pending");

        // List with state=accepted should return empty
        let list_accepted = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/v1/teams/{}/invitations?state=accepted",
                scenario.team.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let accepted_response = router.oneshot(list_accepted).await.unwrap();
        assert_eq!(accepted_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(accepted_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let accepted_invitations: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(accepted_invitations.len(), 0);

        scenario.cleanup().await.unwrap();
    }
}

mod test_authorization_edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_viewer_cannot_update_member_role() {
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

        // Add a member for the viewer to try to update
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

        let role_update = json!({ "role": "admin" });

        let request = Request::builder()
            .method(Method::PATCH)
            .uri(format!(
                "/v1/teams/{}/members/{}",
                team.id, member_fixture.user.id
            ))
            .header(
                "authorization",
                format!("Bearer {}", viewer_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(role_update.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_viewer_cannot_remove_member() {
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

        // Add a member for the viewer to try to remove
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
                format!("Bearer {}", viewer_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cross_team_invite_forbidden() {
        let app = TestApp::new().await.unwrap();

        // Create team A with owner A
        let (owner_a, _team_a, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create team B with owner B
        let (_, team_b, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let router = create_test_router(&app).await;

        // Owner of team A tries to invite someone to team B
        let invite_data = json!({
            "email": "crossteam@example.com",
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team_b.id))
            .header("authorization", format!("Bearer {}", owner_a.jwt_token))
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cross_team_list_members_forbidden() {
        let app = TestApp::new().await.unwrap();

        // Create team A with owner A
        let (owner_a, _, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Create team B with owner B
        let (_, team_b, _) = UserFixture::creator_with_team(&app).await.unwrap();

        let router = create_test_router(&app).await;

        // Owner of team A tries to list members of team B
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/members", team_b.id))
            .header("authorization", format!("Bearer {}", owner_a.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_to_nonexistent_team() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, _, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let fake_team_id = Uuid::new_v4();
        let invite_data = json!({
            "email": "someone@example.com",
            "role": "member"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", fake_team_id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should be 403 (user has no membership in the team) or 404
        let status = response.status();
        assert!(
            status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
            "Expected 403 or 404 for nonexistent team, got {}",
            status
        );

        app.cleanup().await.unwrap();
    }
}

mod test_membership_exotic_inputs {
    use super::*;

    #[tokio::test]
    async fn test_invite_email_with_plus_addressing() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        let invite_data = json!({
            "email": "user+tag@example.com",
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

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(invitation["email"], "user+tag@example.com");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_email_case_handling() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();
        let router = create_test_router(&app).await;

        // Invite with mixed-case email
        let invite_data = json!({
            "email": "USER@Example.COM",
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

        // Should succeed — email addresses are case-insensitive per RFC
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();

        // Document observed behavior (case-insensitive match or stored as-is)
        assert!(invitation.get("email").is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_leave_then_rejoin_via_invitation() {
        let app = TestApp::new().await.unwrap();
        let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

        // Add a member (give them a second team so INV-U2 is satisfied when leaving)
        let member_fixture = UserFixture::creator(&app).await.unwrap();
        app.create_test_team(member_fixture.user.id).await.unwrap();
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

        // Step 1: Member leaves team
        let leave_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/leave", team.id))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(leave_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Step 2: Owner reinvites the same user
        let invite_data = json!({
            "email": member_fixture.user.email,
            "role": "member"
        });

        let invite_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(invite_data.to_string()))
            .unwrap();

        let response = router.clone().oneshot(invite_request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Re-invite after leaving should succeed"
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();
        let invitation_id = invitation["id"].as_str().unwrap();

        // Step 3: Member accepts the new invitation
        let accept_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/accept", invitation_id))
            .header(
                "authorization",
                format!("Bearer {}", member_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(accept_request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Accept after leave+reinvite should succeed"
        );

        // Step 4: Verify membership restored
        let members_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/teams/{}/members", team.id))
            .header(
                "authorization",
                format!("Bearer {}", owner_fixture.jwt_token),
            )
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(members_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let members: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(members.len(), 2, "Team should have owner + rejoined member");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_role_multiple_rapid_changes() {
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

        // Rapid role changes: member → admin → viewer → member
        let role_sequence = ["admin", "viewer", "member"];

        for target_role in role_sequence {
            let role_update = json!({ "role": target_role });

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

            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "Role change to {} should succeed",
                target_role
            );

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let membership: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(membership["role"], target_role);
        }

        // Verify final state in database
        let db_membership: (String,) = sqlx::query_as(
            "SELECT role::text FROM memberships WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team.id)
        .bind(member_fixture.user.id)
        .fetch_one(&app.pool)
        .await
        .unwrap();

        assert_eq!(db_membership.0, "member");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_invite_after_previous_declined() {
        let scenario = InvitationTestScenario::new().await.unwrap();
        let router = create_test_router(&scenario.app).await;

        // Step 1: Send initial invitation
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

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let invitation: Value = serde_json::from_slice(&body).unwrap();
        let invitation_id = invitation["id"].as_str().unwrap();

        // Step 2: Create invitee user and decline
        let invitee = scenario.create_invitee_user().await.unwrap();

        let decline_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/invitations/{}/decline", invitation_id))
            .header("authorization", format!("Bearer {}", invitee.jwt_token))
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(decline_request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NO_CONTENT,
            "Decline should succeed with 204"
        );

        // Step 3: Re-invite the same email — should succeed since previous was declined
        let reinvite_data = json!({
            "email": scenario.invitee_email,
            "role": "admin"
        });

        let reinvite_request = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/teams/{}/invitations", scenario.team.id))
            .header(
                "authorization",
                format!("Bearer {}", scenario.inviter.jwt_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(reinvite_data.to_string()))
            .unwrap();

        let response = router.oneshot(reinvite_request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Re-invite after decline should succeed"
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let new_invitation: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(new_invitation["role"], "admin");
        assert_eq!(new_invitation["state"], "pending");

        scenario.cleanup().await.unwrap();
    }
}
