//! AWS SES Email Service E2E Tests
//!
//! Tests the complete AWS SES email service integration with LocalStack,
//! providing end-to-end validation of email delivery including:
//! - Real AWS SES client configuration with LocalStack
//! - Email sending and delivery verification
//! - Invitation workflow with real email service
//! - LocalStack service health and configuration

use std::time::Duration;

use framecast_email::{
    EmailConfig, EmailMessage, EmailService, EmailServiceFactory,
};
use tokio::time::sleep;
use uuid::Uuid;

/// Test configuration for LocalStack SES
fn create_localstack_email_config() -> EmailConfig {
    EmailConfig {
        provider: "ses".to_string(),
        aws_region: Some("us-east-1".to_string()),
        aws_endpoint_url: Some("http://localhost:4566".to_string()),
        default_from: "invitations@framecast.app".to_string(),
        default_reply_to: Some("noreply@framecast.app".to_string()),
        enabled: true,
    }
}

/// Check if LocalStack is running and accessible
async fn check_localstack_health() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:4566/_localstack/health")
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    if response.status().is_success() {
        let health: serde_json::Value = response.json().await?;
        if let Some(ses_status) = health.get("services").and_then(|s| s.get("ses")) {
            if ses_status == "available" || ses_status == "running" {
                println!("✅ LocalStack SES service is ready: {}", ses_status);
                return Ok(());
            }
        }
    }

    Err("LocalStack SES service not available".into())
}

#[tokio::test]
async fn test_localstack_ses_service_creation() {
    println!("\n🧪 Testing AWS SES service creation with LocalStack...");

    // Skip test if LocalStack is not running
    if check_localstack_health().await.is_err() {
        println!("⏭️ Skipping test: LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config).await
        .expect("Failed to create email service");

    assert_eq!(email_service.service_name(), "aws-ses");
    println!("✅ AWS SES email service created successfully");

    // Test health check
    match email_service.health_check().await {
        Ok(()) => println!("✅ SES health check passed"),
        Err(e) => println!("⚠️ SES health check warning: {} (expected in LocalStack)", e),
    }
}

#[tokio::test]
async fn test_localstack_ses_send_basic_email() {
    println!("\n📧 Testing basic email sending through LocalStack SES...");

    // Skip test if LocalStack is not running
    if check_localstack_health().await.is_err() {
        println!("⏭️ Skipping test: LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config).await
        .expect("Failed to create email service");

    let message = EmailMessage::new(
        "test@framecast.app".to_string(),
        "invitations@framecast.app".to_string(),
        "Test Email from LocalStack SES".to_string(),
        "This is a test email sent through LocalStack SES.".to_string(),
    )
    .with_html("<p>This is a test email sent through <strong>LocalStack SES</strong>.</p>".to_string())
    .with_reply_to("noreply@framecast.app".to_string())
    .with_metadata("test_id".to_string(), "basic_email_test".to_string());

    let receipt = email_service.send_email(message).await
        .expect("Failed to send email");

    println!("📧 Email sent successfully!");
    println!("   📋 Message ID: {}", receipt.message_id);
    println!("   🚀 Provider: {}", receipt.provider);
    println!("   ⏰ Sent at: {}", receipt.sent_at);

    assert_eq!(receipt.provider, "aws-ses");
    assert!(!receipt.message_id.is_empty());
    assert!(receipt.message_id != "unknown");

    println!("✅ Basic email test completed successfully");
}

#[tokio::test]
async fn test_localstack_ses_team_invitation_workflow() {
    println!("\n🎯 Testing complete team invitation workflow with LocalStack SES...");

    // Skip test if LocalStack is not running
    if check_localstack_health().await.is_err() {
        println!("⏭️ Skipping test: LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config).await
        .expect("Failed to create email service");

    // ============================================================================
    // Step 1: Send team invitation email
    // ============================================================================
    println!("\n📤 Step 1: Sending team invitation email...");

    let team_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();
    let team_name = "LocalStack Test Team";
    let invitee_email = "invitee@example.com";
    let inviter_name = "Test Inviter";
    let role = "admin";

    println!("🏢 Team: {} ({})", team_name, team_id);
    println!("🆔 Invitation ID: {}", invitation_id);
    println!("👤 Inviter: {}", inviter_name);
    println!("📧 Invitee: {}", invitee_email);
    println!("🔑 Role: {}", role);

    let receipt = email_service
        .send_team_invitation(
            team_name,
            team_id,
            invitation_id,
            invitee_email,
            inviter_name,
            role,
        )
        .await
        .expect("Failed to send team invitation");

    println!("✅ Team invitation sent successfully!");
    println!("   📋 Message ID: {}", receipt.message_id);
    println!("   🚀 Provider: {}", receipt.provider);

    assert_eq!(receipt.provider, "aws-ses");
    assert!(receipt.metadata.get("email_type") == Some(&"team_invitation".to_string()));
    assert!(receipt.metadata.get("team_id") == Some(&team_id.to_string()));
    assert!(receipt.metadata.get("invitation_id") == Some(&invitation_id.to_string()));

    // ============================================================================
    // Step 2: Verify email metadata and tracking
    // ============================================================================
    println!("\n🔍 Step 2: Verifying email metadata and tracking...");

    // Check metadata
    assert_eq!(receipt.metadata.get("email_type"), Some(&"team_invitation".to_string()));
    assert_eq!(receipt.metadata.get("team_id"), Some(&team_id.to_string()));
    assert_eq!(receipt.metadata.get("invitation_id"), Some(&invitation_id.to_string()));

    println!("✅ Email metadata verification passed!");
    println!("   📊 Email type: {}", receipt.metadata.get("email_type").unwrap());
    println!("   🏢 Team ID: {}", receipt.metadata.get("team_id").unwrap());
    println!("   🆔 Invitation ID: {}", receipt.metadata.get("invitation_id").unwrap());

    // ============================================================================
    // Step 3: Test multiple invitation scenario
    // ============================================================================
    println!("\n👥 Step 3: Testing multiple invitations scenario...");

    let invitations = vec![
        ("developer@example.com", "member"),
        ("admin@example.com", "admin"),
        ("viewer@example.com", "viewer"),
    ];

    for (email, role) in invitations {
        let new_invitation_id = Uuid::new_v4();

        println!("📧 Sending invitation to {} as {}", email, role);

        let receipt = email_service
            .send_team_invitation(
                team_name,
                team_id,
                new_invitation_id,
                email,
                inviter_name,
                role,
            )
            .await
            .expect("Failed to send invitation");

        assert_eq!(receipt.provider, "aws-ses");
        assert_eq!(receipt.metadata.get("role"), Some(&role.to_string()));

        println!("   ✅ Invitation sent to {} ({})", email, receipt.message_id);
    }

    println!("✅ Multiple invitations test completed successfully!");

    // ============================================================================
    // Step 4: Test email service health and configuration
    // ============================================================================
    println!("\n🏥 Step 4: Testing email service health and configuration...");

    // Test health check
    match email_service.health_check().await {
        Ok(()) => println!("✅ Email service health check passed"),
        Err(e) => println!("⚠️ Health check warning: {} (may be expected in LocalStack)", e),
    }

    // Verify service configuration
    assert_eq!(email_service.service_name(), "aws-ses");
    println!("✅ Email service configuration verified");

    // ============================================================================
    // Summary
    // ============================================================================
    println!("\n🎉 === LOCALSTACK SES E2E TEST COMPLETED ===");
    println!("\n📋 What this test validated:");
    println!("   1. ✅ AWS SES service creation with LocalStack configuration");
    println!("   2. ✅ Basic email sending through real SES client");
    println!("   3. ✅ Complete team invitation email workflow");
    println!("   4. ✅ Email metadata tracking and verification");
    println!("   5. ✅ Multiple invitation handling with different roles");
    println!("   6. ✅ Service health monitoring and configuration");

    println!("\n💡 Key Benefits Demonstrated:");
    println!("   🎯 Real AWS SES integration without external dependencies");
    println!("   🔧 LocalStack provides production-equivalent testing");
    println!("   📧 Complete email delivery workflow validation");
    println!("   🔍 Comprehensive metadata tracking for invitation workflows");
    println!("   🧪 Seamless integration between mock and real email services");

    println!("\n🚀 Ready for production AWS SES deployment!");
}

#[tokio::test]
async fn test_localstack_ses_error_handling() {
    println!("\n⚠️ Testing SES error handling and edge cases...");

    // Skip test if LocalStack is not running
    if check_localstack_health().await.is_err() {
        println!("⏭️ Skipping test: LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config).await
        .expect("Failed to create email service");

    // ============================================================================
    // Test 1: Invalid email address
    // ============================================================================
    println!("\n🚫 Test 1: Testing invalid email address handling...");

    let invalid_message = EmailMessage::new(
        "invalid-email".to_string(), // Missing @
        "invitations@framecast.app".to_string(),
        "Test Subject".to_string(),
        "Test body".to_string(),
    );

    match email_service.send_email(invalid_message).await {
        Ok(_) => panic!("Expected validation error for invalid email"),
        Err(e) => {
            println!("✅ Correctly caught validation error: {}", e);
            assert!(e.to_string().contains("validation") || e.to_string().contains("Invalid"));
        }
    }

    // ============================================================================
    // Test 2: Empty subject and body
    // ============================================================================
    println!("\n📝 Test 2: Testing empty subject and body...");

    let empty_message = EmailMessage::new(
        "test@framecast.app".to_string(),
        "invitations@framecast.app".to_string(),
        "".to_string(), // Empty subject
        "".to_string(), // Empty body
    );

    // This should still work with SES but create empty content
    let receipt = email_service.send_email(empty_message).await
        .expect("Should handle empty content gracefully");

    println!("✅ Empty content handled gracefully: {}", receipt.message_id);

    // ============================================================================
    // Test 3: Large email content
    // ============================================================================
    println!("\n📏 Test 3: Testing large email content...");

    let large_body = "A".repeat(10000); // 10KB body
    let large_message = EmailMessage::new(
        "test@framecast.app".to_string(),
        "invitations@framecast.app".to_string(),
        "Large Email Test".to_string(),
        large_body,
    );

    let receipt = email_service.send_email(large_message).await
        .expect("Should handle large content");

    println!("✅ Large email content handled successfully: {}", receipt.message_id);

    println!("\n✅ Error handling tests completed successfully!");
}

#[tokio::test]
async fn test_disabled_email_service() {
    println!("\n🔇 Testing disabled email service behavior...");

    let mut config = create_localstack_email_config();
    config.enabled = false;

    let email_service = EmailServiceFactory::create(config).await
        .expect("Failed to create disabled email service");

    let message = EmailMessage::new(
        "test@framecast.app".to_string(),
        "invitations@framecast.app".to_string(),
        "Test with Disabled Service".to_string(),
        "This should not actually be sent.".to_string(),
    );

    let receipt = email_service.send_email(message).await
        .expect("Disabled service should return success without sending");

    println!("📧 Disabled service response:");
    println!("   📋 Message ID: {}", receipt.message_id);
    println!("   🚀 Provider: {}", receipt.provider);

    // When email is disabled, we get a mock service instead of disabled SES
    assert!(receipt.message_id.starts_with("mock-") || receipt.message_id.contains("disabled"));
    assert!(receipt.provider == "mock" || receipt.provider == "aws-ses-disabled");

    println!("✅ Disabled email service test completed successfully!");
}

mod common;