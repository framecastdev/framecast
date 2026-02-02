//! Mock Email Service Implementation
//!
//! Provides in-memory email capture for testing without external dependencies.
//! Compatible with the integration test infrastructure and can capture
//! invitation emails for workflow validation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{EmailError, EmailMessage, EmailReceipt, EmailService};

/// Email captured by the mock service
#[derive(Debug, Clone)]
pub struct CapturedEmail {
    pub message: EmailMessage,
    pub receipt: EmailReceipt,
    pub captured_at: DateTime<Utc>,
}

impl CapturedEmail {
    /// Extract invitation ID from email content using regex patterns
    pub fn extract_invitation_id(&self) -> Option<Uuid> {
        // First check metadata
        if let Some(invitation_id_str) = self.message.metadata.get("invitation_id") {
            if let Ok(uuid) = Uuid::parse_str(invitation_id_str) {
                return Some(uuid);
            }
        }

        // Try to extract from URL patterns in email body
        let text = format!(
            "{} {}",
            self.message.body_text,
            self.message.body_html.as_deref().unwrap_or("")
        );

        // Look for patterns like /invitations/{uuid}/accept or invitation_id={uuid}
        let patterns = [
            r"/invitations/([0-9a-f-]{36})/accept",
            r"invitation_id=([0-9a-f-]{36})",
            r"invite/([0-9a-f-]{36})",
        ];

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(captures) = re.captures(&text) {
                    if let Some(uuid_str) = captures.get(1) {
                        if let Ok(uuid) = Uuid::parse_str(uuid_str.as_str()) {
                            return Some(uuid);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract team ID from email content
    pub fn extract_team_id(&self) -> Option<Uuid> {
        // First check metadata
        if let Some(team_id_str) = self.message.metadata.get("team_id") {
            if let Ok(uuid) = Uuid::parse_str(team_id_str) {
                return Some(uuid);
            }
        }

        // Try to extract from URL patterns
        let text = format!(
            "{} {}",
            self.message.body_text,
            self.message.body_html.as_deref().unwrap_or("")
        );

        let pattern = r"/teams/([0-9a-f-]{36})/";
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(&text) {
                if let Some(uuid_str) = captures.get(1) {
                    if let Ok(uuid) = Uuid::parse_str(uuid_str.as_str()) {
                        return Some(uuid);
                    }
                }
            }
        }

        None
    }
}

/// Mock email service for testing
#[derive(Debug, Clone)]
pub struct MockEmailService {
    emails: Arc<Mutex<Vec<CapturedEmail>>>,
    email_by_recipient: Arc<Mutex<HashMap<String, Vec<CapturedEmail>>>>,
    enabled: bool,
}

impl MockEmailService {
    /// Create a new mock email service
    pub fn new() -> Self {
        Self {
            emails: Arc::new(Mutex::new(Vec::new())),
            email_by_recipient: Arc::new(Mutex::new(HashMap::new())),
            enabled: true,
        }
    }

    /// Create a disabled mock email service (for testing)
    pub fn new_disabled() -> Self {
        Self {
            emails: Arc::new(Mutex::new(Vec::new())),
            email_by_recipient: Arc::new(Mutex::new(HashMap::new())),
            enabled: false,
        }
    }

    /// Get all captured emails
    pub fn get_all_emails(&self) -> Vec<CapturedEmail> {
        self.emails.lock().unwrap().clone()
    }

    /// Get emails sent to a specific recipient
    pub fn get_emails_for_recipient(&self, email: &str) -> Vec<CapturedEmail> {
        self.email_by_recipient
            .lock()
            .unwrap()
            .get(email)
            .cloned()
            .unwrap_or_default()
    }

    /// Get the most recent invitation email for a recipient
    pub fn get_latest_invitation_email(&self, email: &str) -> Option<CapturedEmail> {
        self.get_emails_for_recipient(email)
            .into_iter()
            .filter(|e| {
                e.message
                    .metadata
                    .get("email_type")
                    .map(|t| t == "team_invitation")
                    .unwrap_or(false)
                    || e.message.subject.to_lowercase().contains("invitation")
            })
            .max_by_key(|e| e.captured_at)
    }

    /// Get invitation ID from the most recent invitation email
    pub fn get_invitation_id_for_email(&self, email: &str) -> Option<Uuid> {
        self.get_latest_invitation_email(email)
            .and_then(|email| email.extract_invitation_id())
    }

    /// Check if an invitation email was sent to a specific email address
    pub fn was_invitation_sent_to(&self, email: &str) -> bool {
        self.get_invitation_id_for_email(email).is_some()
    }

    /// Get count of emails sent
    pub fn email_count(&self) -> usize {
        self.emails.lock().unwrap().len()
    }

    /// Clear all captured emails
    pub fn clear(&self) {
        self.emails.lock().unwrap().clear();
        self.email_by_recipient.lock().unwrap().clear();
    }

    /// Set enabled state
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if email sending is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for MockEmailService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EmailService for MockEmailService {
    async fn send_email(&self, message: EmailMessage) -> Result<EmailReceipt, EmailError> {
        if !self.enabled {
            tracing::warn!("Mock email service disabled, skipping send");
            return Ok(EmailReceipt {
                message_id: format!("disabled-{}", Uuid::new_v4()),
                sent_at: Utc::now(),
                provider: "mock-disabled".to_string(),
                metadata: message.metadata.clone(),
            });
        }

        tracing::info!("Mock email service capturing email to: {}", message.to);

        let receipt = EmailReceipt {
            message_id: format!("mock-{}", Uuid::new_v4()),
            sent_at: Utc::now(),
            provider: "mock".to_string(),
            metadata: message.metadata.clone(),
        };

        let captured = CapturedEmail {
            message: message.clone(),
            receipt: receipt.clone(),
            captured_at: Utc::now(),
        };

        // Store email in global list
        self.emails.lock().unwrap().push(captured.clone());

        // Store email by recipient for easy lookup
        self.email_by_recipient
            .lock()
            .unwrap()
            .entry(message.to)
            .or_default()
            .push(captured);

        tracing::info!(
            "Email captured successfully, message ID: {}",
            receipt.message_id
        );

        Ok(receipt)
    }

    async fn send_team_invitation(
        &self,
        team_name: &str,
        team_id: Uuid,
        invitation_id: Uuid,
        recipient_email: &str,
        inviter_name: &str,
        role: &str,
    ) -> Result<EmailReceipt, EmailError> {
        tracing::info!(
            "Mock service sending team invitation to {} for team {} ({})",
            recipient_email,
            team_name,
            team_id
        );

        let invitation_url = format!(
            "https://framecast.app/teams/{}/invitations/{}/accept",
            team_id, invitation_id
        );

        let subject = format!("Invitation to join team: {}", team_name);

        let body_text = format!(
            "Hi there!\n\n\
            {} has invited you to join the team '{}' as a {}.\n\n\
            Click the link below to accept the invitation:\n\
            {}\n\n\
            This invitation will expire in 7 days.\n\n\
            If you don't have a Framecast account, you'll be prompted to create one.\n\n\
            Thanks,\n\
            The Framecast Team",
            inviter_name, team_name, role, invitation_url
        );

        let body_html = format!(
            r#"
            <html>
            <body>
                <h2>You're invited to join {team_name}!</h2>
                <p>Hi there!</p>
                <p><strong>{inviter_name}</strong> has invited you to join the team '<strong>{team_name}</strong>' as a <strong>{role}</strong>.</p>
                <p>
                    <a href="{invitation_url}" style="background-color: #007cba; color: white; padding: 12px 24px; text-decoration: none; border-radius: 4px; display: inline-block;">
                        Accept Invitation
                    </a>
                </p>
                <p>Or copy and paste this link in your browser:</p>
                <p><a href="{invitation_url}">{invitation_url}</a></p>
                <p><small>This invitation will expire in 7 days.</small></p>
                <hr>
                <p><small>If you don't have a Framecast account, you'll be prompted to create one.</small></p>
                <p><small>Thanks, The Framecast Team</small></p>
            </body>
            </html>
            "#,
            team_name = team_name,
            inviter_name = inviter_name,
            role = role,
            invitation_url = invitation_url
        );

        let message = EmailMessage::new(
            recipient_email.to_string(),
            "invitations@framecast.app".to_string(),
            subject,
            body_text,
        )
        .with_html(body_html)
        .with_metadata("email_type".to_string(), "team_invitation".to_string())
        .with_metadata("team_id".to_string(), team_id.to_string())
        .with_metadata("invitation_id".to_string(), invitation_id.to_string())
        .with_metadata("role".to_string(), role.to_string());

        self.send_email(message).await
    }

    fn service_name(&self) -> &'static str {
        "mock"
    }

    async fn health_check(&self) -> Result<(), EmailError> {
        // Mock service is always healthy
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_email_service() {
        let service = MockEmailService::new();

        let message = EmailMessage::new(
            "test@example.com".to_string(),
            "sender@framecast.app".to_string(),
            "Test Subject".to_string(),
            "Test body".to_string(),
        );

        let receipt = service.send_email(message).await.unwrap();

        assert!(receipt.message_id.starts_with("mock-"));
        assert_eq!(receipt.provider, "mock");
        assert_eq!(service.email_count(), 1);

        let emails = service.get_emails_for_recipient("test@example.com");
        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0].message.subject, "Test Subject");
    }

    #[tokio::test]
    async fn test_team_invitation_email() {
        let service = MockEmailService::new();
        let team_id = Uuid::new_v4();
        let invitation_id = Uuid::new_v4();

        let receipt = service
            .send_team_invitation(
                "Test Team",
                team_id,
                invitation_id,
                "invitee@example.com",
                "Inviter User",
                "member",
            )
            .await
            .unwrap();

        assert_eq!(receipt.provider, "mock");

        let captured = service
            .get_latest_invitation_email("invitee@example.com")
            .unwrap();
        assert_eq!(captured.extract_invitation_id(), Some(invitation_id));
        assert_eq!(captured.extract_team_id(), Some(team_id));

        assert!(service.was_invitation_sent_to("invitee@example.com"));
        assert_eq!(
            service.get_invitation_id_for_email("invitee@example.com"),
            Some(invitation_id)
        );
    }

    #[test]
    fn test_invitation_id_extraction() {
        let _service = MockEmailService::new();

        let message = EmailMessage::new(
            "test@example.com".to_string(),
            "sender@framecast.app".to_string(),
            "Team Invitation".to_string(),
            "Click here: https://framecast.app/teams/550e8400-e29b-41d4-a716-446655440001/invitations/550e8400-e29b-41d4-a716-446655440000/accept".to_string(), // pragma: allowlist secret
        );

        let captured = CapturedEmail {
            message,
            receipt: EmailReceipt {
                message_id: "test".to_string(),
                sent_at: Utc::now(),
                provider: "test".to_string(),
                metadata: HashMap::new(),
            },
            captured_at: Utc::now(),
        };

        let extracted_id = captured.extract_invitation_id();
        assert!(extracted_id.is_some());
        assert_eq!(
            extracted_id.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        ); // pragma: allowlist secret
    }

    #[tokio::test]
    async fn test_disabled_mock_service() {
        let service = MockEmailService::new_disabled();

        let message = EmailMessage::new(
            "test@example.com".to_string(),
            "sender@framecast.app".to_string(),
            "Test Subject".to_string(),
            "Test body".to_string(),
        );

        let receipt = service.send_email(message).await.unwrap();

        assert!(receipt.message_id.starts_with("disabled-"));
        assert_eq!(receipt.provider, "mock-disabled");
        assert_eq!(service.email_count(), 0); // Email not captured when disabled
    }
}
