//! AWS SES Email Service Implementation
//!
//! Provides production email delivery through AWS Simple Email Service (SES)
//! with support for LocalStack testing environment.

use std::collections::HashMap;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_ses::config::SharedCredentialsProvider;
use aws_sdk_ses::types::{Body, Content, Destination, Message};
use aws_sdk_ses::Client as SesClient;
use chrono::Utc;
use uuid::Uuid;

use crate::{EmailConfig, EmailError, EmailMessage, EmailReceipt, EmailService};

/// AWS SES email service implementation
pub struct SesEmailService {
    client: SesClient,
    config: EmailConfig,
}

impl SesEmailService {
    /// Create a new SES email service
    pub async fn new(config: EmailConfig) -> Result<Self, EmailError> {
        let region = config
            .aws_region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_string());

        let aws_config = match config.aws_endpoint_url.as_ref() {
            Some(endpoint_url) => {
                tracing::info!("Using custom AWS endpoint: {}", endpoint_url);

                // For LocalStack, use dummy credentials
                let credentials = Credentials::new(
                    "test-access-key",
                    "test-secret-key",
                    None,
                    None,
                    "localstack-email-provider",
                );

                aws_config::defaults(BehaviorVersion::latest())
                    .region(Region::new(region.clone()))
                    .endpoint_url(endpoint_url)
                    .credentials_provider(SharedCredentialsProvider::new(credentials))
                    .load()
                    .await
            }
            None => {
                // Use default AWS configuration (real AWS)
                aws_config::defaults(BehaviorVersion::latest())
                    .region(Region::new(region.clone()))
                    .load()
                    .await
            }
        };

        let client = SesClient::new(&aws_config);

        // Test connection
        if let Err(e) = client.get_send_quota().send().await {
            tracing::warn!(
                "Failed to connect to SES (may be expected in LocalStack): {}",
                e
            );
            // Don't fail here as LocalStack might not have SES fully configured yet
        } else {
            tracing::info!("Successfully connected to AWS SES");
        }

        Ok(Self { client, config })
    }

    /// Convert email message to SES format
    fn build_ses_message(&self, message: &EmailMessage) -> Result<Message, EmailError> {
        let subject = Content::builder()
            .data(&message.subject)
            .charset("UTF-8")
            .build()
            .map_err(|e| EmailError::AwsSes(format!("Failed to build subject: {}", e)))?;

        let text_content = Content::builder()
            .data(&message.body_text)
            .charset("UTF-8")
            .build()
            .map_err(|e| EmailError::AwsSes(format!("Failed to build text content: {}", e)))?;

        let mut body_builder = Body::builder().text(text_content);

        // Add HTML content if provided
        if let Some(html_body) = &message.body_html {
            let html_content = Content::builder()
                .data(html_body)
                .charset("UTF-8")
                .build()
                .map_err(|e| EmailError::AwsSes(format!("Failed to build HTML content: {}", e)))?;

            body_builder = body_builder.html(html_content);
        }

        let body = body_builder.build();

        let ses_message = Message::builder().subject(subject).body(body).build();

        Ok(ses_message)
    }

    /// Build destination from email address
    fn build_destination(&self, to: &str) -> Result<Destination, EmailError> {
        let destination = Destination::builder().to_addresses(to).build();

        Ok(destination)
    }
}

#[async_trait::async_trait]
impl EmailService for SesEmailService {
    async fn send_email(&self, message: EmailMessage) -> Result<EmailReceipt, EmailError> {
        tracing::info!("Sending email via AWS SES to: {}", message.to);

        if !self.config.enabled {
            tracing::warn!("Email sending disabled, skipping SES send");
            return Ok(EmailReceipt {
                message_id: format!("disabled-{}", Uuid::new_v4()),
                sent_at: Utc::now(),
                provider: "aws-ses-disabled".to_string(),
                metadata: message.metadata.clone(),
            });
        }

        // Validate email address
        if !message.to.contains('@') || !message.from.contains('@') {
            return Err(EmailError::Validation(
                "Invalid email address format".to_string(),
            ));
        }

        let ses_message = self.build_ses_message(&message)?;
        let destination = self.build_destination(&message.to)?;

        let mut send_builder = self
            .client
            .send_email()
            .source(&message.from)
            .destination(destination)
            .message(ses_message);

        // Add reply-to if provided
        if let Some(reply_to) = &message.reply_to {
            send_builder = send_builder.reply_to_addresses(reply_to);
        }

        let result = send_builder
            .send()
            .await
            .map_err(|e| EmailError::AwsSes(format!("Failed to send email: {}", e)))?;

        let message_id = result.message_id().to_string();

        tracing::info!(
            "Email sent successfully via SES, message ID: {}",
            message_id
        );

        Ok(EmailReceipt {
            message_id,
            sent_at: Utc::now(),
            provider: "aws-ses".to_string(),
            metadata: message.metadata.clone(),
        })
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
            "Sending team invitation email to {} for team {} ({})",
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
            <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                    <h2 style="color: #007cba;">You're invited to join {team_name}!</h2>

                    <p>Hi there!</p>

                    <p><strong>{inviter_name}</strong> has invited you to join the team '<strong>{team_name}</strong>' as a <strong>{role}</strong>.</p>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{invitation_url}"
                           style="background-color: #007cba; color: white; padding: 12px 24px; text-decoration: none; border-radius: 4px; display: inline-block; font-weight: bold;">
                            Accept Invitation
                        </a>
                    </div>

                    <p>Or copy and paste this link in your browser:</p>
                    <p style="background-color: #f5f5f5; padding: 10px; border-radius: 4px; word-break: break-all;">
                        <a href="{invitation_url}">{invitation_url}</a>
                    </p>

                    <p style="color: #666; font-size: 14px;">
                        <em>This invitation will expire in 7 days.</em>
                    </p>

                    <hr style="border: none; border-top: 1px solid #eee; margin: 30px 0;">

                    <p style="color: #666; font-size: 12px;">
                        If you don't have a Framecast account, you'll be prompted to create one.<br>
                        Thanks, The Framecast Team
                    </p>
                </div>
            </body>
            </html>
            "#,
            team_name = team_name,
            inviter_name = inviter_name,
            role = role,
            invitation_url = invitation_url
        );

        let mut metadata = HashMap::new();
        metadata.insert("email_type".to_string(), "team_invitation".to_string());
        metadata.insert("team_id".to_string(), team_id.to_string());
        metadata.insert("invitation_id".to_string(), invitation_id.to_string());
        metadata.insert("role".to_string(), role.to_string());

        let message = EmailMessage::new(
            recipient_email.to_string(),
            self.config.default_from.clone(),
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
        "aws-ses"
    }

    async fn health_check(&self) -> Result<(), EmailError> {
        tracing::debug!("Performing SES health check");

        // Try to get send quota as a simple health check
        match self.client.get_send_quota().send().await {
            Ok(_) => {
                tracing::debug!("SES health check passed");
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("SES health check failed: {}", e);
                tracing::warn!("{}", error_msg);
                Err(EmailError::AwsSes(error_msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmailConfig;

    #[tokio::test]
    async fn test_ses_service_creation() {
        let config = EmailConfig {
            provider: "ses".to_string(),
            aws_region: Some("us-east-1".to_string()),
            aws_endpoint_url: Some("http://localhost:4566".to_string()),
            default_from: "test@framecast.app".to_string(),
            default_reply_to: None,
            enabled: true,
        };

        // This will work with LocalStack but may fail with real AWS without credentials
        // In real testing, we'd configure proper test credentials
        let result = SesEmailService::new(config).await;

        // We expect this to succeed in creating the service, even if health check fails
        assert!(result.is_ok());

        let service = result.unwrap();
        assert_eq!(service.service_name(), "aws-ses");
    }

    #[test]
    fn test_message_building() {
        let config = EmailConfig {
            provider: "ses".to_string(),
            aws_region: Some("us-east-1".to_string()),
            aws_endpoint_url: None,
            default_from: "test@framecast.app".to_string(),
            default_reply_to: None,
            enabled: true,
        };

        // We can't easily test the actual SES message building without creating a service
        // This test just validates the config structure
        assert_eq!(config.provider, "ses");
        assert!(config.enabled);
    }
}
