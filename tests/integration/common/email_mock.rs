//! Mock email service for testing invitation workflows
//!
//! This module provides a mock email service that captures emails sent during
//! invitation processes and extracts invitation codes for testing purposes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Email message captured by the mock service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockEmail {
    pub to: String,
    pub from: String,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub sent_at: DateTime<Utc>,
    pub invitation_id: Option<Uuid>,
    pub invitation_code: Option<String>,
}

#[allow(dead_code)]
impl MockEmail {
    /// Extract invitation ID from email content
    pub fn extract_invitation_id(&mut self) -> Option<Uuid> {
        if let Some(invitation_id) = self.invitation_id {
            return Some(invitation_id);
        }

        // Try to extract from URL patterns in email body
        let text = format!(
            "{} {}",
            self.body_text,
            self.body_html.as_deref().unwrap_or("")
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
                            self.invitation_id = Some(uuid);
                            return Some(uuid);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract invitation acceptance code from email content
    pub fn extract_invitation_code(&mut self) -> Option<String> {
        if let Some(ref code) = self.invitation_code {
            return Some(code.clone());
        }

        // Try to extract from URL parameters or email body
        let text = format!(
            "{} {}",
            self.body_text,
            self.body_html.as_deref().unwrap_or("")
        );

        // Look for patterns like code=ABC123 or verification code: ABC123
        let patterns = [
            r"code=([A-Z0-9]{6,})",
            r"verification code[:\s]+([A-Z0-9]{6,})",
            r"invitation code[:\s]+([A-Z0-9]{6,})",
            r"access code[:\s]+([A-Z0-9]{6,})",
        ];

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(captures) = re.captures(&text) {
                    if let Some(code) = captures.get(1) {
                        let code_str = code.as_str().to_string();
                        self.invitation_code = Some(code_str.clone());
                        return Some(code_str);
                    }
                }
            }
        }

        None
    }
}

/// Mock email service that captures and stores emails
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MockEmailService {
    emails: Arc<Mutex<Vec<MockEmail>>>,
    email_by_recipient: Arc<Mutex<HashMap<String, Vec<MockEmail>>>>,
    webhook_delivery_enabled: bool,
}

#[allow(dead_code)]
impl MockEmailService {
    /// Create a new mock email service
    pub fn new() -> Self {
        Self {
            emails: Arc::new(Mutex::new(Vec::new())),
            email_by_recipient: Arc::new(Mutex::new(HashMap::new())),
            webhook_delivery_enabled: true,
        }
    }

    /// Send an email (mock implementation that captures the email)
    pub async fn send_email(&self, email: MockEmail) -> Result<(), String> {
        let mut emails = self
            .emails
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        let mut by_recipient = self
            .email_by_recipient
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;

        // Store email in global list
        emails.push(email.clone());

        // Store email by recipient for easy lookup
        by_recipient
            .entry(email.to.clone())
            .or_insert_with(Vec::new)
            .push(email);

        Ok(())
    }

    /// Send team invitation email
    pub async fn send_invitation_email(
        &self,
        team_name: &str,
        team_id: Uuid,
        invitation_id: Uuid,
        recipient_email: &str,
        inviter_name: &str,
        role: &str,
    ) -> Result<(), String> {
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

        let mut email = MockEmail {
            to: recipient_email.to_string(),
            from: "invitations@framecast.app".to_string(),
            subject,
            body_text,
            body_html: Some(body_html),
            sent_at: Utc::now(),
            invitation_id: Some(invitation_id),
            invitation_code: None,
        };

        // Extract invitation data for testing
        email.extract_invitation_id();

        self.send_email(email).await
    }

    /// Get all emails sent to a specific recipient
    pub fn get_emails_for_recipient(&self, email: &str) -> Vec<MockEmail> {
        self.email_by_recipient
            .lock()
            .unwrap()
            .get(email)
            .cloned()
            .unwrap_or_default()
    }

    /// Get the most recent invitation email for a recipient
    pub fn get_latest_invitation_email(&self, email: &str) -> Option<MockEmail> {
        self.get_emails_for_recipient(email)
            .into_iter()
            .filter(|e| e.invitation_id.is_some())
            .max_by_key(|e| e.sent_at)
    }

    /// Get invitation ID from the most recent invitation email
    pub fn get_invitation_id_for_email(&self, email: &str) -> Option<Uuid> {
        self.get_latest_invitation_email(email)
            .and_then(|mut email| email.extract_invitation_id())
    }

    /// Get all emails (for debugging)
    pub fn get_all_emails(&self) -> Vec<MockEmail> {
        self.emails.lock().unwrap().clone()
    }

    /// Clear all emails (for test cleanup)
    pub fn clear(&self) {
        self.emails.lock().unwrap().clear();
        self.email_by_recipient.lock().unwrap().clear();
    }

    /// Get count of emails sent
    pub fn email_count(&self) -> usize {
        self.emails.lock().unwrap().len()
    }

    /// Check if an invitation email was sent to a specific email address
    pub fn was_invitation_sent_to(&self, email: &str) -> bool {
        self.get_invitation_id_for_email(email).is_some()
    }

    /// Simulate webhook delivery (for integration with Inngest/webhook system)
    pub async fn trigger_webhook_delivery(
        &self,
        _team_id: Uuid,
        _invitation_id: Uuid,
    ) -> Result<(), String> {
        if !self.webhook_delivery_enabled {
            return Ok(());
        }

        // In a real system, this would trigger an Inngest event or webhook
        // For testing, we simulate the email sending process

        // Find the invitation in the database and send the email
        // This is a simplified mock - in reality, this would be triggered by
        // a webhook event after the invitation is created

        Ok(())
    }

    /// Enable/disable webhook delivery simulation
    pub fn set_webhook_delivery_enabled(&mut self, enabled: bool) {
        self.webhook_delivery_enabled = enabled;
    }
}

impl Default for MockEmailService {
    fn default() -> Self {
        Self::new()
    }
}

/// Test utilities for invitation email workflows
#[allow(dead_code)]
pub mod test_utils {
    use super::*;
    use crate::common::{TestApp, UserFixture};
    use framecast_domain::entities::InvitationRole;

    /// Create a complete invitation test scenario
    pub struct InvitationTestScenario {
        pub app: TestApp,
        pub email_service: MockEmailService,
        pub inviter: UserFixture,
        pub team: framecast_domain::entities::Team,
        pub invitee_email: String,
    }

    impl InvitationTestScenario {
        /// Set up a complete invitation test scenario
        pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let app = TestApp::new().await?;
            let email_service = MockEmailService::new();

            // Create inviter (creator user with team)
            let (inviter, team, _membership) = UserFixture::creator_with_team(&app).await?;

            // Generate invitee email
            let invitee_email = format!("invitee_{}@test.example", uuid::Uuid::new_v4().simple());

            Ok(Self {
                app,
                email_service,
                inviter,
                team,
                invitee_email,
            })
        }

        /// Send invitation and return the invitation ID from captured email
        pub async fn send_invitation(
            &self,
            role: InvitationRole,
        ) -> Result<Uuid, Box<dyn std::error::Error>> {
            // Create invitation in database (simulating API call)
            let invitation = framecast_domain::entities::Invitation::new(
                self.team.id,
                self.inviter.user.id,
                self.invitee_email.clone(),
                role.clone(),
            )?;

            // Store invitation in database
            let created_invitation = self.app.state.repos.invitations.create(&invitation).await?;

            // Send email through mock service
            self.email_service
                .send_invitation_email(
                    &self.team.name,
                    self.team.id,
                    created_invitation.id,
                    &self.invitee_email,
                    self.inviter.user.name.as_deref().unwrap_or("Unknown"),
                    &format!("{:?}", role).to_lowercase(),
                )
                .await?;

            Ok(created_invitation.id)
        }

        /// Create invitee user and get their auth fixture
        pub async fn create_invitee_user(&self) -> Result<UserFixture, Box<dyn std::error::Error>> {
            // Create creator user with the invitee email
            let mut invitee_user = framecast_domain::entities::User::new(
                Uuid::new_v4(),
                self.invitee_email.clone(),
                Some("Invitee User".to_string()),
            )?;

            // Upgrade to creator (required for team membership per INV-M4)
            invitee_user.upgrade_to_creator()?;

            // Clone tier before move
            let user_tier = invitee_user.tier.clone();

            // Insert into database (using runtime query to avoid sqlx offline mode issues in tests)
            sqlx::query(
                r#"
                INSERT INTO users (id, email, name, tier, credits, ephemeral_storage_bytes, upgraded_at, created_at, updated_at)
                VALUES ($1, $2, $3, $4::user_tier, $5, $6, $7, $8, $9)
                "#,
            )
            .bind(invitee_user.id)
            .bind(&invitee_user.email)
            .bind(&invitee_user.name)
            .bind(user_tier.to_string())
            .bind(invitee_user.credits)
            .bind(invitee_user.ephemeral_storage_bytes)
            .bind(invitee_user.upgraded_at)
            .bind(invitee_user.created_at)
            .bind(invitee_user.updated_at)
            .execute(&self.app.pool).await?;

            let jwt_token =
                crate::common::create_test_jwt(&invitee_user, &self.app.config.jwt_secret)?;

            Ok(UserFixture {
                user: invitee_user,
                jwt_token,
            })
        }

        /// Complete invitation workflow: send invitation, create user, accept invitation
        pub async fn complete_invitation_workflow(
            &self,
            role: InvitationRole,
        ) -> Result<(Uuid, UserFixture), Box<dyn std::error::Error>> {
            // Send invitation
            let invitation_id = self.send_invitation(role).await?;

            // Verify email was sent
            assert!(self
                .email_service
                .was_invitation_sent_to(&self.invitee_email));
            let captured_invitation_id = self
                .email_service
                .get_invitation_id_for_email(&self.invitee_email);
            assert_eq!(captured_invitation_id, Some(invitation_id));

            // Create invitee user
            let invitee = self.create_invitee_user().await?;

            Ok((invitation_id, invitee))
        }

        /// Clean up test data
        pub async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
            self.email_service.clear();
            self.app.cleanup().await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_extraction() {
        let mut email = MockEmail {
            to: "test@example.com".to_string(),
            from: "invitations@framecast.app".to_string(),
            subject: "Team Invitation".to_string(),
            body_text: "Click here: https://framecast.app/teams/123/invitations/550e8400-e29b-41d4-a716-446655440000/accept".to_string(), // pragma: allowlist secret
            body_html: None,
            sent_at: Utc::now(),
            invitation_id: None,
            invitation_code: None,
        };

        let extracted_id = email.extract_invitation_id();
        assert!(extracted_id.is_some());
        assert_eq!(
            extracted_id.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440000" // pragma: allowlist secret
        );
    }

    #[tokio::test]
    async fn test_mock_email_service() {
        let service = MockEmailService::new();

        let email = MockEmail {
            to: "test@example.com".to_string(),
            from: "noreply@framecast.app".to_string(),
            subject: "Test Email".to_string(),
            body_text: "Test body".to_string(),
            body_html: None,
            sent_at: Utc::now(),
            invitation_id: None,
            invitation_code: None,
        };

        service.send_email(email).await.unwrap();

        assert_eq!(service.email_count(), 1);
        assert_eq!(
            service.get_emails_for_recipient("test@example.com").len(),
            1
        );
    }

    #[tokio::test]
    async fn test_invitation_email_sending() {
        let service = MockEmailService::new();
        let team_id = Uuid::new_v4();
        let invitation_id = Uuid::new_v4();

        service
            .send_invitation_email(
                "Test Team",
                team_id,
                invitation_id,
                "invitee@example.com",
                "Inviter User",
                "member",
            )
            .await
            .unwrap();

        assert!(service.was_invitation_sent_to("invitee@example.com"));

        let captured_id = service.get_invitation_id_for_email("invitee@example.com");
        assert_eq!(captured_id, Some(invitation_id));
    }
}
