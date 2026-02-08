//! Mock Email Service Implementation
//!
//! Minimal mock used by `EmailServiceFactory` when provider is `"mock"` or
//! email is disabled. Integration tests define their own richer mock in
//! `tests/integration/common/email_mock.rs`.

use chrono::Utc;
use uuid::Uuid;

use crate::{EmailError, EmailMessage, EmailReceipt, EmailService};

/// Mock email service for testing
#[derive(Debug, Clone)]
pub struct MockEmailService;

impl MockEmailService {
    /// Create a new mock email service
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockEmailService {
    fn default() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl EmailService for MockEmailService {
    async fn send_email(&self, message: EmailMessage) -> Result<EmailReceipt, EmailError> {
        tracing::info!("Mock email service capturing email to: {}", message.to);

        let receipt = EmailReceipt {
            message_id: format!("mock-{}", Uuid::new_v4()),
            sent_at: Utc::now(),
            provider: "mock".to_string(),
            metadata: message.metadata.clone(),
        };

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
        let body_text =
            crate::content::team_invitation_text(inviter_name, team_name, role, &invitation_url);
        let body_html =
            crate::content::team_invitation_html(inviter_name, team_name, role, &invitation_url);

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
        assert_eq!(
            receipt.metadata.get("email_type"),
            Some(&"team_invitation".to_string())
        );
        assert_eq!(
            receipt.metadata.get("invitation_id"),
            Some(&invitation_id.to_string())
        );
    }
}
