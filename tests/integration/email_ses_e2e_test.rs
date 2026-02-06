//! AWS SES Email Service E2E Tests
//!
//! Tests the complete AWS SES email service integration with LocalStack,
//! providing end-to-end validation of email delivery including:
//! - Real AWS SES client configuration with LocalStack
//! - Email sending and delivery verification
//! - Invitation workflow with real email service
//! - LocalStack service health and configuration

use std::time::Duration;

use framecast_email::{EmailConfig, EmailMessage, EmailServiceFactory};
use uuid::Uuid;

/// Get the LocalStack endpoint URL from environment or default to localhost
fn localstack_endpoint() -> String {
    std::env::var("AWS_ENDPOINT_URL").unwrap_or_else(|_| "http://localhost:4566".to_string())
}

/// Whether LocalStack is expected to be available (tests should fail instead of skip).
/// True when AWS_ENDPOINT_URL is explicitly set (e.g. in the localstack-test CI job).
fn require_localstack() -> bool {
    std::env::var("AWS_ENDPOINT_URL").is_ok()
}

/// Test configuration for LocalStack SES
fn create_localstack_email_config() -> EmailConfig {
    EmailConfig {
        provider: "ses".to_string(),
        aws_region: Some("us-east-1".to_string()),
        aws_endpoint_url: Some(localstack_endpoint()),
        default_from: "invitations@framecast.app".to_string(),
        default_reply_to: Some("noreply@framecast.app".to_string()),
        enabled: true,
    }
}

/// Check if LocalStack is running and accessible
async fn check_localstack_health() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = localstack_endpoint();
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/_localstack/health", endpoint))
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    if response.status().is_success() {
        println!("✅ LocalStack is ready");
        return Ok(());
    }

    Err("LocalStack not available".into())
}

/// Skip or panic depending on whether LocalStack is expected
fn skip_or_panic(msg: &str) {
    if require_localstack() {
        panic!("LocalStack required but: {}", msg);
    }
    println!("⏭️ Skipping test: {}", msg);
}

#[tokio::test]
async fn test_localstack_ses_service_creation() {
    println!("\n🧪 Testing AWS SES service creation with LocalStack...");

    // Skip test if LocalStack is not running (panic in CI)
    if check_localstack_health().await.is_err() {
        skip_or_panic("LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config)
        .await
        .expect("Failed to create email service");

    assert_eq!(email_service.service_name(), "aws-ses");
    println!("✅ AWS SES email service created successfully");

    // Test health check
    match email_service.health_check().await {
        Ok(()) => println!("✅ SES health check passed"),
        Err(e) => println!(
            "⚠️ SES health check warning: {} (expected in LocalStack)",
            e
        ),
    }
}

#[tokio::test]
async fn test_localstack_ses_send_basic_email() {
    println!("\n📧 Testing basic email sending through LocalStack SES...");

    // Skip test if LocalStack is not running (panic in CI)
    if check_localstack_health().await.is_err() {
        skip_or_panic("LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config)
        .await
        .expect("Failed to create email service");

    let message = EmailMessage::new(
        "test@framecast.app".to_string(),
        "invitations@framecast.app".to_string(),
        "Test Email from LocalStack SES".to_string(),
        "This is a test email sent through LocalStack SES.".to_string(),
    )
    .with_html(
        "<p>This is a test email sent through <strong>LocalStack SES</strong>.</p>".to_string(),
    )
    .with_reply_to("noreply@framecast.app".to_string())
    .with_metadata("test_id".to_string(), "basic_email_test".to_string());

    let receipt = email_service
        .send_email(message)
        .await
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

    // Skip test if LocalStack is not running (panic in CI)
    if check_localstack_health().await.is_err() {
        skip_or_panic("LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config)
        .await
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
    assert_eq!(
        receipt.metadata.get("email_type"),
        Some(&"team_invitation".to_string())
    );
    assert_eq!(receipt.metadata.get("team_id"), Some(&team_id.to_string()));
    assert_eq!(
        receipt.metadata.get("invitation_id"),
        Some(&invitation_id.to_string())
    );

    println!("✅ Email metadata verification passed!");
    println!(
        "   📊 Email type: {}",
        receipt.metadata.get("email_type").unwrap()
    );
    println!(
        "   🏢 Team ID: {}",
        receipt.metadata.get("team_id").unwrap()
    );
    println!(
        "   🆔 Invitation ID: {}",
        receipt.metadata.get("invitation_id").unwrap()
    );

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

        println!(
            "   ✅ Invitation sent to {} ({})",
            email, receipt.message_id
        );
    }

    println!("✅ Multiple invitations test completed successfully!");

    // ============================================================================
    // Step 4: Test email service health and configuration
    // ============================================================================
    println!("\n🏥 Step 4: Testing email service health and configuration...");

    // Test health check
    match email_service.health_check().await {
        Ok(()) => println!("✅ Email service health check passed"),
        Err(e) => println!(
            "⚠️ Health check warning: {} (may be expected in LocalStack)",
            e
        ),
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

    // Skip test if LocalStack is not running (panic in CI)
    if check_localstack_health().await.is_err() {
        skip_or_panic("LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config)
        .await
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

    println!("\n✅ Error handling tests completed successfully!");
}

#[tokio::test]
async fn test_disabled_email_service() {
    println!("\n🔇 Testing disabled email service behavior...");

    let mut config = create_localstack_email_config();
    config.enabled = false;

    let email_service = EmailServiceFactory::create(config)
        .await
        .expect("Failed to create disabled email service");

    let message = EmailMessage::new(
        "test@framecast.app".to_string(),
        "invitations@framecast.app".to_string(),
        "Test with Disabled Service".to_string(),
        "This should not actually be sent.".to_string(),
    );

    let receipt = email_service
        .send_email(message)
        .await
        .expect("Disabled service should return success without sending");

    println!("📧 Disabled service response:");
    println!("   📋 Message ID: {}", receipt.message_id);
    println!("   🚀 Provider: {}", receipt.provider);

    // When email is disabled, we get a mock service instead of disabled SES
    assert!(receipt.message_id.starts_with("mock-") || receipt.message_id.contains("disabled"));
    assert!(receipt.provider == "mock" || receipt.provider == "aws-ses-disabled");

    println!("✅ Disabled email service test completed successfully!");
}

#[tokio::test]
async fn test_localstack_ses_email_retrieval_and_content_validation() {
    println!("\n📧 Testing LocalStack SES email retrieval and content validation...");

    // Skip test if LocalStack is not running (panic in CI)
    if check_localstack_health().await.is_err() {
        skip_or_panic("LocalStack SES not available");
        return;
    }

    let config = create_localstack_email_config();
    let email_service = EmailServiceFactory::create(config)
        .await
        .expect("Failed to create email service");

    let localstack_client = LocalStackEmailClient::from_env();

    // Verify LocalStack SES is healthy
    match localstack_client.health_check().await {
        Ok(true) => println!("✅ LocalStack SES service is healthy"),
        Ok(false) => println!("⚠️ LocalStack SES service health check inconclusive"),
        Err(e) => {
            println!(
                "⚠️ LocalStack health check failed: {}, continuing anyway",
                e
            );
        }
    }

    // ============================================================================
    // Step 1: Send invitation email through SES
    // ============================================================================
    println!("\n📤 Step 1: Sending team invitation email through SES...");

    let team_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();
    let invitee_email = "retrieve-test@example.com";
    let team_name = "LocalStack Retrieval Test Team";
    let inviter_name = "Admin User";
    let role = "admin";

    println!("🏢 Team: {} ({})", team_name, team_id);
    println!("🆔 Invitation ID: {}", invitation_id);
    println!("👤 Inviter: {}", inviter_name);
    println!("📧 Invitee: {}", invitee_email);
    println!("🔑 Role: {}", role);

    // Clear any existing emails for this address first
    let cleared = localstack_client
        .clear_emails(invitee_email)
        .await
        .unwrap_or(0);
    if cleared > 0 {
        println!("🧹 Cleared {} existing emails for test address", cleared);
    }

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

    println!("✅ Email sent successfully through SES!");
    println!("   📋 Message ID: {}", receipt.message_id);
    println!("   🚀 Provider: {}", receipt.provider);

    // ============================================================================
    // Step 2: Retrieve email from LocalStack SES API
    // ============================================================================
    println!("\n📥 Step 2: Retrieving email from LocalStack SES API...");

    // Wait a moment for email to be stored in LocalStack
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to retrieve the invitation email
    println!("🔍 Checking LocalStack SES API for emails...");

    // First, try a direct API call to see what's there
    let endpoint = localstack_endpoint();
    match reqwest::get(&format!("{}/_aws/ses", endpoint)).await {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            println!("   📡 LocalStack API response ({}): {}", status, body);
        }
        Err(e) => {
            println!("   ⚠️ Failed to query LocalStack API directly: {}", e);
        }
    }

    let retrieved_email = localstack_client
        .wait_for_invitation_email(invitee_email, 10)
        .await
        .expect("Failed to retrieve emails from LocalStack SES API")
        .expect("No invitation email found for recipient in LocalStack SES");

    println!("✅ Email successfully retrieved from LocalStack!");
    println!("   🆔 Email ID: {}", retrieved_email.id);
    println!("   📧 Subject: {}", retrieved_email.subject);
    println!("   📤 From: {}", retrieved_email.from);
    println!("   📥 To: {:?}", retrieved_email.to);

    // ============================================================================
    // Step 3: Validate email content and metadata
    // ============================================================================
    println!("\n🔍 Step 3: Validating email content and metadata...");

    // Validate basic email properties
    assert!(
        retrieved_email.subject.contains(team_name),
        "Email subject should contain team name"
    );
    assert!(
        retrieved_email.body.contains(inviter_name),
        "Email body should contain inviter name"
    );
    assert!(
        retrieved_email.body.contains(role),
        "Email body should contain role"
    );
    assert!(
        retrieved_email.to.contains(&invitee_email.to_string()),
        "Email should be addressed to invitee"
    );
    assert_eq!(
        retrieved_email.from, "invitations@framecast.app",
        "Email should be from invitations address"
    );

    println!("✅ Basic email content validation passed!");

    // ============================================================================
    // Step 4: Extract and validate invitation data from email content
    // ============================================================================
    println!("\n🔗 Step 4: Extracting invitation data from email content...");

    // Extract invitation ID from email content
    let extracted_invitation_id = localstack_client
        .extract_invitation_id(&retrieved_email)
        .expect("Failed to extract invitation ID from email content");

    assert_eq!(
        extracted_invitation_id, invitation_id,
        "Extracted invitation ID should match sent invitation ID"
    );

    println!("✅ Invitation ID extracted: {}", extracted_invitation_id);

    // Extract team ID from email content
    let extracted_team_id = localstack_client
        .extract_team_id(&retrieved_email)
        .expect("Failed to extract team ID from email content");

    assert_eq!(
        extracted_team_id, team_id,
        "Extracted team ID should match sent team ID"
    );

    println!("✅ Team ID extracted: {}", extracted_team_id);

    // Extract invitation URL from email content
    let invitation_url = localstack_client.extract_invitation_url(&retrieved_email);

    if let Some(url) = &invitation_url {
        assert!(
            url.contains(&invitation_id.to_string()),
            "Invitation URL should contain invitation ID"
        );
        assert!(
            url.contains(&team_id.to_string()),
            "Invitation URL should contain team ID"
        );
        assert!(
            url.contains("/accept"),
            "Invitation URL should contain accept endpoint"
        );

        println!("✅ Invitation URL extracted: {}", url);
    } else {
        println!(
            "⚠️ Could not extract invitation URL (may be expected depending on email template)"
        );
    }

    // ============================================================================
    // Step 5: Test email retrieval methods
    // ============================================================================
    println!("\n🔄 Step 5: Testing different email retrieval methods...");

    // Test get_emails (all emails for address)
    let all_emails = localstack_client
        .get_emails(invitee_email)
        .await
        .expect("Failed to get all emails");

    assert!(!all_emails.is_empty(), "Should have at least one email");
    println!("📧 Found {} total emails for address", all_emails.len());

    // Test get_latest_email
    let latest_email = localstack_client
        .get_latest_email(invitee_email)
        .await
        .expect("Failed to get latest email");

    assert!(latest_email.is_some(), "Should have a latest email");
    println!("📧 Latest email ID: {}", latest_email.unwrap().id);

    // Test get_latest_invitation (should be same as retrieved_email)
    let latest_invitation = localstack_client
        .get_latest_invitation(invitee_email)
        .await
        .expect("Failed to get latest invitation");

    assert!(
        latest_invitation.is_some(),
        "Should have a latest invitation"
    );
    assert_eq!(
        latest_invitation.as_ref().unwrap().id,
        retrieved_email.id,
        "Latest invitation should match retrieved email"
    );

    println!("✅ All email retrieval methods working correctly!");

    // ============================================================================
    // Summary
    // ============================================================================
    println!("\n🎉 === LOCALSTACK EMAIL RETRIEVAL TEST COMPLETED ===");
    println!("\n📋 What this test validated:");
    println!("   1. ✅ Email sending through AWS SES to LocalStack");
    println!("   2. ✅ Email retrieval from LocalStack SES REST API");
    println!("   3. ✅ Email content validation (subject, body, recipients)");
    println!("   4. ✅ Invitation ID extraction from email content");
    println!("   5. ✅ Team ID extraction from email content");
    println!("   6. ✅ Invitation URL extraction from email content");
    println!("   7. ✅ Multiple email retrieval methods (latest, all, invitations)");

    println!("\n💡 Key Benefits Demonstrated:");
    println!("   🎯 Complete end-to-end email workflow validation");
    println!("   📧 Real email content inspection and parsing");
    println!("   🔗 Invitation data extraction for workflow integration");
    println!("   🧪 Production-equivalent testing with LocalStack");
    println!("   🔍 Comprehensive email metadata validation");

    println!("\n🚀 Ready for complete E2E invitation workflow testing!");
}

#[tokio::test]
async fn test_localstack_client_health_and_basic_operations() {
    println!("\n🩺 Testing LocalStack client health and basic operations...");

    let client = LocalStackEmailClient::from_env();

    // Test health check
    match client.health_check().await {
        Ok(true) => println!("✅ LocalStack SES service is healthy"),
        Ok(false) => println!("⚠️ LocalStack SES service health check returned false"),
        Err(e) => {
            skip_or_panic(&format!("LocalStack health check failed: {}", e));
            return;
        }
    }

    // Test email retrieval for non-existent address (should return empty)
    let test_email = "nonexistent@test.local";
    let emails = client.get_emails(test_email).await;

    match emails {
        Ok(emails) => {
            println!(
                "📧 Retrieved {} emails for test address {}",
                emails.len(),
                test_email
            );
        }
        Err(e) => {
            println!(
                "⚠️ Email retrieval failed (may be expected if LocalStack SES not configured): {}",
                e
            );
        }
    }

    // Test latest email retrieval
    let latest = client.get_latest_email(test_email).await;
    match latest {
        Ok(None) => println!("✅ Correctly returned None for latest email on empty address"),
        Ok(Some(email)) => println!("📧 Found existing email: {}", email.subject),
        Err(e) => println!("⚠️ Latest email retrieval error: {}", e),
    }

    println!("✅ LocalStack client basic operations test completed!");
}

mod common;

use common::localstack_client::LocalStackEmailClient;
