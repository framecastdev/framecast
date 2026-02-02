//! Invitation Workflow Integration Tests
//!
//! Tests the complete invitation email mock system
//! without requiring database setup, validating email capture
//! and invitation ID extraction functionality.

use chrono::Utc;
use uuid::Uuid;

use crate::common::email_mock::{MockEmail, MockEmailService};

#[tokio::test]
async fn test_invitation_email_workflow_e2e() {
    println!("\n🚀 === INVITATION EMAIL MOCK WORKFLOW TEST ===\n");

    // ============================================================================
    // Step 1: Setup Mock Email Service
    // ============================================================================
    println!("📧 Step 1: Setting up mock email service...");

    let email_service = MockEmailService::new();
    println!("✅ Mock email service initialized and ready");

    // ============================================================================
    // Step 2: Simulate API Invitation Creation
    // ============================================================================
    println!("\n📝 Step 2: Simulating API invitation creation...");

    let team_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();
    let team_name = "Awesome Development Team";
    let invitee_email = "newdev@company.com";
    let inviter_name = "Sarah Johnson";
    let role = "admin";

    println!("🏢 Team: {} ({})", team_name, team_id);
    println!("🆔 Invitation ID: {}", invitation_id);
    println!("👤 Inviter: {}", inviter_name);
    println!("📧 Invitee: {}", invitee_email);
    println!("🔑 Role: {}", role);

    // ============================================================================
    // Step 3: Send Invitation Email (Mock)
    // ============================================================================
    println!("\n📮 Step 3: Sending invitation email through mock service...");

    let result = email_service
        .send_invitation_email(
            team_name,
            team_id,
            invitation_id,
            invitee_email,
            inviter_name,
            role,
        )
        .await;

    match result {
        Ok(()) => println!("✅ Invitation email sent successfully!"),
        Err(e) => panic!("❌ Failed to send invitation email: {}", e),
    }

    // ============================================================================
    // Step 4: Verify Email Capture
    // ============================================================================
    println!("\n🔍 Step 4: Verifying email capture and content...");

    // Check if email was sent to the recipient
    let was_sent = email_service.was_invitation_sent_to(invitee_email);
    println!(
        "📧 Email sent to recipient: {}",
        if was_sent { "✅ YES" } else { "❌ NO" }
    );
    assert!(was_sent, "Email should have been sent to recipient");

    // Get the captured email
    let captured_email = email_service.get_latest_invitation_email(invitee_email);
    assert!(
        captured_email.is_some(),
        "Should have captured an invitation email"
    );

    let email = captured_email.unwrap();
    println!("📬 Captured Email Details:");
    println!("   📧 To: {}", email.to);
    println!("   📧 From: {}", email.from);
    println!("   📝 Subject: {}", email.subject);
    println!(
        "   📅 Sent At: {}",
        email.sent_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    // Verify email content
    assert_eq!(email.to, invitee_email);
    assert_eq!(email.from, "invitations@framecast.app");
    assert!(email.subject.contains(team_name));
    assert!(email.body_text.contains(inviter_name));
    assert!(email.body_text.contains(team_name));
    assert!(email.body_text.contains(role));

    println!("✅ Email content verification passed!");

    // ============================================================================
    // Step 5: Automatic Invitation ID Extraction
    // ============================================================================
    println!("\n🔍 Step 5: Testing automatic invitation ID extraction...");

    // Get the invitation ID extracted by the mock service
    let extracted_id = email_service.get_invitation_id_for_email(invitee_email);

    println!("🆔 Original Invitation ID: {}", invitation_id);
    println!("🔍 Extracted Invitation ID: {:?}", extracted_id);

    assert!(
        extracted_id.is_some(),
        "Should have extracted invitation ID"
    );
    assert_eq!(
        extracted_id.unwrap(),
        invitation_id,
        "Extracted ID should match original"
    );

    println!("✅ Invitation ID extraction successful!");

    // ============================================================================
    // Step 6: Demonstrate Email Content Analysis
    // ============================================================================
    println!("\n📊 Step 6: Analyzing email content for invitation data...");

    // Show the actual email content that was generated
    println!("📄 Email Text Content (truncated):");
    let content_preview = if email.body_text.len() > 200 {
        format!("{}...", &email.body_text[..200])
    } else {
        email.body_text.clone()
    };

    for (i, line) in content_preview.lines().enumerate() {
        if i < 8 {
            // Show first 8 lines
            println!("   {}", line);
        }
    }

    if email.body_html.is_some() {
        println!("📄 HTML Email Content: ✅ Available");
    }

    // ============================================================================
    // Step 7: Simulate Multiple Invitations
    // ============================================================================
    println!("\n👥 Step 7: Testing multiple invitation scenario...");

    let second_invitee = "developer@company.com";
    let second_invitation_id = Uuid::new_v4();

    email_service
        .send_invitation_email(
            team_name,
            team_id,
            second_invitation_id,
            second_invitee,
            inviter_name,
            "member",
        )
        .await
        .unwrap();

    let third_invitee = "designer@company.com";
    let third_invitation_id = Uuid::new_v4();

    email_service
        .send_invitation_email(
            team_name,
            team_id,
            third_invitation_id,
            third_invitee,
            inviter_name,
            "viewer",
        )
        .await
        .unwrap();

    println!("📧 Total emails sent: {}", email_service.email_count());

    // Verify each email has correct invitation ID
    let id_2 = email_service.get_invitation_id_for_email(second_invitee);
    let id_3 = email_service.get_invitation_id_for_email(third_invitee);

    assert_eq!(id_2, Some(second_invitation_id));
    assert_eq!(id_3, Some(third_invitation_id));

    println!("✅ Multiple invitation tracking working correctly!");

    // ============================================================================
    // Step 8: Show Complete Email Service State
    // ============================================================================
    println!("\n📊 Step 8: Email service final state summary...");

    println!("📈 Final Statistics:");
    println!("   📧 Total emails sent: {}", email_service.email_count());
    println!("   👥 Recipients: {}", 3);
    println!("   🆔 Invitation IDs tracked: {}", 3);

    // Verify all invitation IDs are captured correctly
    let all_recipients = [invitee_email, second_invitee, third_invitee];
    let expected_ids = [invitation_id, second_invitation_id, third_invitation_id];

    for (recipient, expected_id) in all_recipients.iter().zip(expected_ids.iter()) {
        let captured_id = email_service.get_invitation_id_for_email(recipient);
        assert_eq!(
            captured_id,
            Some(*expected_id),
            "ID mismatch for {}: expected {:?}, got {:?}",
            recipient,
            expected_id,
            captured_id
        );
        println!("   ✅ {}: {}", recipient, expected_id);
    }

    // ============================================================================
    // Summary
    // ============================================================================
    println!("\n🎉 === INVITATION EMAIL MOCK TEST COMPLETED ===");
    println!("\n📋 What this test validated:");
    println!("   1. ✅ Mock email service setup and initialization");
    println!("   2. ✅ Complete invitation email generation with proper content");
    println!("   3. ✅ Automatic email capture and storage by recipient");
    println!("   4. ✅ Invitation ID extraction from email URLs using regex");
    println!("   5. ✅ Email content validation (team name, inviter, role)");
    println!("   6. ✅ Multiple invitation handling and ID tracking");
    println!("   7. ✅ Complete email service state management");

    println!("\n💡 Key Features Tested:");
    println!("   🎯 No external email service required for testing");
    println!("   🔍 Automatic invitation ID extraction from email content");
    println!("   📧 Rich email content generation matching production format");
    println!("   🔗 Seamless integration with acceptance workflow testing");
    println!("   📊 Complete email tracking and verification capabilities");
    println!("   🧪 Perfect for both unit and integration testing scenarios");

    println!("\n🚀 Ready for Integration with Real API Endpoints!");
    println!("   This mock system can be integrated with actual API tests");
    println!("   to provide end-to-end invitation workflow testing.");

    println!("\n✨ Test completed successfully! ✨");
}

#[tokio::test]
async fn test_email_content_regex_extraction() {
    println!("\n🔍 === EMAIL CONTENT REGEX EXTRACTION TEST ===\n");

    println!("🧪 Testing invitation ID extraction from various email formats...\n");

    let test_cases = vec![
        (
            "Standard URL format",
            "Click here: https://framecast.app/teams/550e8400-e29b-41d4-a716-446655440001/invitations/550e8400-e29b-41d4-a716-446655440000/accept", // pragma: allowlist secret
            "550e8400-e29b-41d4-a716-446655440000" // pragma: allowlist secret
        ),
        (
            "Query parameter format",
            "Visit: https://framecast.app/accept?invitation_id=550e8400-e29b-41d4-a716-446655440000&team=123", // pragma: allowlist secret
            "550e8400-e29b-41d4-a716-446655440000" // pragma: allowlist secret
        ),
        (
            "Short URL format",
            "Accept invitation: https://framecast.app/invite/550e8400-e29b-41d4-a716-446655440000", // pragma: allowlist secret
            "550e8400-e29b-41d4-a716-446655440000" // pragma: allowlist secret
        ),
    ];

    for (description, email_content, expected_uuid) in test_cases {
        println!("📧 Testing: {}", description);

        let mut email = MockEmail {
            to: "test@example.com".to_string(),
            from: "invitations@framecast.app".to_string(),
            subject: "Team Invitation".to_string(),
            body_text: email_content.to_string(),
            body_html: None,
            sent_at: Utc::now(),
            invitation_id: None,
            invitation_code: None,
        };

        let extracted = email.extract_invitation_id();
        println!(
            "   📧 Content: ...{}",
            &email_content[email_content.len().saturating_sub(80)..]
        );
        println!("   🎯 Expected: {}", expected_uuid);
        println!("   🔍 Extracted: {:?}", extracted);

        assert!(
            extracted.is_some(),
            "Should extract invitation ID from: {}",
            description
        );
        assert_eq!(
            extracted.unwrap().to_string(),
            expected_uuid,
            "Extracted ID should match expected for: {}",
            description
        );

        println!("   ✅ Success!\n");
    }

    println!("🎉 All regex extraction tests passed!");
}

mod common;
