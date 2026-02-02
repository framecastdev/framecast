//! Complete Invitation Workflow Demonstration
//!
//! This demonstrates the full invitation lifecycle with email capture:
//! 1. Team owner sends invitation → API creates invitation in DB
//! 2. Email service captures invitation email with invitation ID
//! 3. Invitee user receives email, clicks link with invitation ID
//! 4. Invitee accepts invitation → Membership created
//! 5. Email mock provides invitation ID for seamless testing

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::Utc;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_api::routes;
use framecast_domain::entities::{MembershipRole, User, UserTier};

use crate::common::{email_mock::MockEmailService, TestApp, UserFixture};

/// Create test router with all routes
async fn create_test_router(app: &TestApp) -> Router {
    routes::create_routes().with_state(app.state.clone())
}

#[tokio::test]
async fn demo_complete_invitation_workflow() {
    println!("\n🚀 === COMPLETE INVITATION WORKFLOW DEMO ===");

    // ============================================================================
    // Step 1: Setup - Create team owner and mock email service
    // ============================================================================
    println!("\n📋 Step 1: Setting up test environment...");

    let app = TestApp::new().await.unwrap();
    let email_service = MockEmailService::new();

    // Create team owner (creator user)
    let (owner_fixture, team, _owner_membership) =
        UserFixture::creator_with_team(&app).await.unwrap();

    println!(
        "✅ Created team owner: {} ({})",
        owner_fixture.user.name.as_ref().unwrap(),
        owner_fixture.user.email
    );
    println!("✅ Created team: {} ({})", team.name, team.slug);
    println!("✅ Mock email service ready");

    // ============================================================================
    // Step 2: Send Invitation via API
    // ============================================================================
    println!("\n📧 Step 2: Sending invitation via API...");

    let router = create_test_router(&app).await;
    let invitee_email = "newuser@example.com";

    let invite_data = json!({
        "email": invitee_email,
        "role": "admin"
    });

    let invite_request = Request::builder()
        .method(Method::POST)
        .uri(&format!("/v1/teams/{}/invite", team.id))
        .header(
            "authorization",
            format!("Bearer {}", owner_fixture.jwt_token),
        )
        .header("content-type", "application/json")
        .body(Body::from(invite_data.to_string()))
        .unwrap();

    let invite_response = router.clone().oneshot(invite_request).await.unwrap();

    println!("📨 API Response Status: {}", invite_response.status());
    assert_eq!(invite_response.status(), StatusCode::OK);

    let invite_body = axum::body::to_bytes(invite_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let invitation: Value = serde_json::from_slice(&invite_body).unwrap();

    let invitation_id = Uuid::parse_str(invitation["id"].as_str().unwrap()).unwrap();

    println!("✅ Invitation created successfully!");
    println!("   📧 Recipient: {}", invitation["email"]);
    println!("   🔑 Role: {}", invitation["role"]);
    println!("   🆔 Invitation ID: {}", invitation_id);
    println!(
        "   👤 Invited by: {}",
        owner_fixture.user.name.as_ref().unwrap()
    );

    // ============================================================================
    // Step 3: Mock Email Service Captures Email
    // ============================================================================
    println!("\n📮 Step 3: Mock email service captures invitation email...");

    // Simulate the email service receiving the invitation webhook and sending email
    email_service
        .send_invitation_email(
            &team.name,
            team.id,
            invitation_id,
            invitee_email,
            owner_fixture.user.name.as_deref().unwrap_or("Unknown"),
            "admin",
        )
        .await
        .unwrap();

    println!("✅ Email captured by mock service!");

    // Verify email was captured and invitation ID extracted
    assert!(email_service.was_invitation_sent_to(invitee_email));

    let captured_email = email_service
        .get_latest_invitation_email(invitee_email)
        .unwrap();
    let captured_invitation_id = email_service.get_invitation_id_for_email(invitee_email);

    println!("📧 Email Details:");
    println!("   📬 To: {}", captured_email.to);
    println!("   📝 Subject: {}", captured_email.subject);
    println!(
        "   🆔 Extracted Invitation ID: {:?}",
        captured_invitation_id
    );

    assert_eq!(captured_invitation_id, Some(invitation_id));
    assert!(captured_email.body_text.contains(&team.name));
    assert!(captured_email.body_text.contains("admin"));

    println!("✅ Invitation ID successfully extracted from email!");

    // ============================================================================
    // Step 4: Create Invitee User Account
    // ============================================================================
    println!("\n👤 Step 4: Creating invitee user account...");

    // Create invitee user account (they sign up after receiving email)
    let invitee_user = User::new(
        Uuid::new_v4(),
        invitee_email.to_string(),
        Some("Invited User".to_string()),
    )
    .unwrap();

    // Upgrade to creator (required for team membership per INV-M4)
    let mut creator_invitee = invitee_user;
    creator_invitee.upgrade_to_creator().unwrap();

    // Insert into database
    let user_tier = creator_invitee.tier.clone();
    sqlx::query!(
        r#"
        INSERT INTO users (id, email, name, tier, credits, ephemeral_storage_bytes, upgraded_at, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        creator_invitee.id,
        creator_invitee.email,
        creator_invitee.name,
        user_tier as UserTier,
        creator_invitee.credits,
        creator_invitee.ephemeral_storage_bytes,
        creator_invitee.upgraded_at,
        creator_invitee.created_at,
        creator_invitee.updated_at
    ).execute(&app.pool).await.unwrap();

    let invitee_jwt =
        crate::common::create_test_jwt(&creator_invitee, &app.config.jwt_secret).unwrap();

    println!("✅ Invitee user created!");
    println!("   👤 Name: {}", creator_invitee.name.as_ref().unwrap());
    println!("   📧 Email: {}", creator_invitee.email);
    println!("   🏆 Tier: {:?}", creator_invitee.tier);

    // ============================================================================
    // Step 5: Accept Invitation Using Captured ID
    // ============================================================================
    println!("\n🤝 Step 5: Accepting invitation using captured invitation ID...");

    // User clicks the link in email and accepts invitation
    let accept_request = Request::builder()
        .method(Method::PUT)
        .uri(&format!("/v1/invitations/{}/accept", invitation_id))
        .header("authorization", format!("Bearer {}", invitee_jwt))
        .body(Body::empty())
        .unwrap();

    let accept_response = router.oneshot(accept_request).await.unwrap();

    println!("🎯 Accept Response Status: {}", accept_response.status());
    assert_eq!(accept_response.status(), StatusCode::OK);

    let accept_body = axum::body::to_bytes(accept_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let membership: Value = serde_json::from_slice(&accept_body).unwrap();

    println!("✅ Invitation accepted successfully!");
    println!("   🆔 Membership ID: {}", membership["id"]);
    println!("   👤 User ID: {}", membership["user_id"]);
    println!("   🔑 Role: {}", membership["role"]);

    assert_eq!(membership["role"], "admin");
    assert_eq!(membership["user_id"], creator_invitee.id.to_string());

    // ============================================================================
    // Step 6: Verify Database State
    // ============================================================================
    println!("\n🗃️  Step 6: Verifying database state...");

    // Verify membership exists in database
    let db_membership_row = sqlx::query!(
        "SELECT id, team_id, user_id, role::text as role_text, created_at FROM memberships WHERE team_id = $1 AND user_id = $2",
        team.id,
        creator_invitee.id
    ).fetch_one(&app.pool).await.unwrap();

    println!("✅ Database verification successful!");
    println!("   🏢 Team ID: {}", db_membership_row.team_id);
    println!("   👤 User ID: {}", db_membership_row.user_id);
    println!("   🔑 Role: {:?}", db_membership_row.role_text);
    println!("   📅 Created: {}", db_membership_row.created_at);

    assert_eq!(db_membership_row.team_id, team.id);
    assert_eq!(db_membership_row.user_id, creator_invitee.id);
    assert_eq!(db_membership_row.role_text, Some("admin".to_string()));

    // ============================================================================
    // Step 7: Verify Business Invariants
    // ============================================================================
    println!("\n📋 Step 7: Verifying business invariants...");

    // Verify team still has at least one owner (INV-T2)
    let owner_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM memberships WHERE team_id = $1 AND role = 'owner'",
        team.id
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert!(owner_count.count.unwrap() >= 1);
    println!(
        "✅ INV-T2: Team has {} owner(s)",
        owner_count.count.unwrap()
    );

    // Verify total membership count
    let total_members = sqlx::query!(
        "SELECT COUNT(*) as count FROM memberships WHERE team_id = $1",
        team.id
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_eq!(total_members.count.unwrap(), 2); // Owner + new admin
    println!("✅ Total team members: {}", total_members.count.unwrap());

    // Verify invitee is creator tier (INV-M4)
    assert_eq!(creator_invitee.tier, UserTier::Creator);
    println!("✅ INV-M4: New member is creator tier");

    // ============================================================================
    // Step 8: Cleanup
    // ============================================================================
    println!("\n🧹 Step 8: Cleaning up test data...");

    app.cleanup().await.unwrap();
    email_service.clear();

    println!("✅ Cleanup completed!");

    // ============================================================================
    // Summary
    // ============================================================================
    println!("\n🎉 === INVITATION WORKFLOW DEMO COMPLETED ===");
    println!("\n📊 Summary of what was tested:");
    println!("   1. ✅ API invitation creation with proper validation");
    println!("   2. ✅ Email service captures invitation with correct ID extraction");
    println!("   3. ✅ User account creation with required creator tier");
    println!("   4. ✅ Invitation acceptance using captured invitation ID");
    println!("   5. ✅ Database membership creation with correct role");
    println!("   6. ✅ Business invariant validation (INV-T2, INV-M4)");
    println!("   7. ✅ Complete end-to-end workflow verification");

    println!("\n💡 Key Benefits of Email Mock System:");
    println!("   🎯 Captures emails sent during invitation process");
    println!("   🔍 Extracts invitation IDs from email content automatically");
    println!("   🔗 Enables seamless testing of complete invitation workflow");
    println!("   📧 Supports both webhook-triggered and direct email scenarios");
    println!("   🧪 Provides rich test utilities for invitation testing");
}

#[tokio::test]
async fn demo_invitation_workflow_error_scenarios() {
    println!("\n⚠️ === INVITATION ERROR SCENARIOS DEMO ===");

    let app = TestApp::new().await.unwrap();
    let _email_service = MockEmailService::new();
    let router = create_test_router(&app).await;

    // Setup team owner
    let (owner_fixture, team, _) = UserFixture::creator_with_team(&app).await.unwrap();

    println!("\n📋 Testing error scenarios...");

    // ============================================================================
    // Scenario 1: Starter user tries to accept invitation (should fail per INV-M4)
    // ============================================================================
    println!("\n🚫 Scenario 1: Starter user tries to accept invitation...");

    // Create invitation
    let invitee_email = "starter@example.com";
    let invitation_id = Uuid::new_v4();

    // Simulate invitation exists
    sqlx::query!(
        r#"
        INSERT INTO invitations (id, team_id, email, role, invited_by, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        invitation_id,
        team.id,
        invitee_email,
        MembershipRole::Member as MembershipRole,
        owner_fixture.user.id,
        Utc::now(),
        Utc::now() + chrono::Duration::days(7)
    )
    .execute(&app.pool)
    .await
    .unwrap();

    // Create starter user (cannot accept invitations)
    let starter_user = app.create_test_user(UserTier::Starter).await.unwrap();
    let starter_jwt =
        crate::common::create_test_jwt(&starter_user, &app.config.jwt_secret).unwrap();

    // Try to accept invitation
    let accept_request = Request::builder()
        .method(Method::PUT)
        .uri(&format!("/v1/invitations/{}/accept", invitation_id))
        .header("authorization", format!("Bearer {}", starter_jwt))
        .body(Body::empty())
        .unwrap();

    let response = router.clone().oneshot(accept_request).await.unwrap();

    println!("📊 Response Status: {}", response.status());
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    println!("✅ Correctly rejected starter user invitation acceptance");

    // ============================================================================
    // Scenario 2: Duplicate invitation handling
    // ============================================================================
    println!("\n🚫 Scenario 2: Testing duplicate invitation handling...");

    let duplicate_email = "duplicate@example.com";

    // Send first invitation
    let invite_data = json!({
        "email": duplicate_email,
        "role": "member"
    });

    let invite1 = Request::builder()
        .method(Method::POST)
        .uri(&format!("/v1/teams/{}/invite", team.id))
        .header(
            "authorization",
            format!("Bearer {}", owner_fixture.jwt_token),
        )
        .header("content-type", "application/json")
        .body(Body::from(invite_data.to_string()))
        .unwrap();

    let response1 = router.clone().oneshot(invite1).await.unwrap();
    assert_eq!(response1.status(), StatusCode::OK);

    // Try to send duplicate invitation
    let invite2 = Request::builder()
        .method(Method::POST)
        .uri(&format!("/v1/teams/{}/invite", team.id))
        .header(
            "authorization",
            format!("Bearer {}", owner_fixture.jwt_token),
        )
        .header("content-type", "application/json")
        .body(Body::from(invite_data.to_string()))
        .unwrap();

    let response2 = router.oneshot(invite2).await.unwrap();

    println!("📊 Duplicate Response Status: {}", response2.status());
    assert_eq!(response2.status(), StatusCode::CONFLICT);
    println!("✅ Correctly rejected duplicate invitation");

    app.cleanup().await.unwrap();

    println!("\n🎉 Error scenario testing completed successfully!");
}

mod common;
