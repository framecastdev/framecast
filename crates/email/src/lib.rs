//! Framecast Email Service
//!
//! Provides email functionality for invitation workflows with support for:
//! - AWS SES integration for production email delivery
//! - Mock email service for testing and development
//! - LocalStack integration for local E2E testing
//! - Comprehensive invitation email templates and tracking

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub mod aws_ses;
pub mod content;
pub mod mock;

#[derive(Error, Debug)]
pub enum EmailError {
    #[error("Email configuration error: {0}")]
    Configuration(String),

    #[error("Email validation error: {0}")]
    Validation(String),

    #[error("AWS SES error: {0}")]
    AwsSes(String),
}

/// Email message to be sent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub to: String,
    pub from: String,
    pub reply_to: Option<String>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl EmailMessage {
    /// Create a new email message
    pub fn new(to: String, from: String, subject: String, body_text: String) -> Self {
        Self {
            to,
            from,
            reply_to: None,
            subject,
            body_text,
            body_html: None,
            metadata: HashMap::new(),
        }
    }

    /// Add HTML body content
    pub fn with_html(mut self, body_html: String) -> Self {
        self.body_html = Some(body_html);
        self
    }

    /// Add reply-to address
    pub fn with_reply_to(mut self, reply_to: String) -> Self {
        self.reply_to = Some(reply_to);
        self
    }

    /// Add metadata for tracking
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Email delivery receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailReceipt {
    pub message_id: String,
    pub sent_at: DateTime<Utc>,
    pub provider: String,
    pub metadata: HashMap<String, String>,
}

/// Email service configuration
#[derive(Debug, Clone)]
pub struct EmailConfig {
    /// Email service provider (ses, mock)
    pub provider: String,
    /// AWS region for SES
    pub aws_region: Option<String>,
    /// AWS endpoint URL (for LocalStack)
    pub aws_endpoint_url: Option<String>,
    /// Default from address
    pub default_from: String,
    /// Enable email sending (can disable for testing)
    pub enabled: bool,
    /// Base URL for the application (used in invitation links)
    pub app_base_url: String,
}

impl EmailConfig {
    /// Create email config from environment variables
    pub fn from_env() -> Result<Self, EmailError> {
        dotenvy::dotenv().ok();

        let provider = std::env::var("EMAIL_PROVIDER").unwrap_or_else(|_| "mock".to_string());

        let aws_region = std::env::var("AWS_REGION").ok();
        let aws_endpoint_url = std::env::var("AWS_ENDPOINT_URL").ok();

        let default_from =
            std::env::var("FROM_EMAIL").unwrap_or_else(|_| "invitations@framecast.app".to_string());

        let enabled = std::env::var("EMAIL_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let app_base_url =
            std::env::var("APP_BASE_URL").unwrap_or_else(|_| "https://framecast.app".to_string());

        Ok(Self {
            provider,
            aws_region,
            aws_endpoint_url,
            default_from,
            enabled,
            app_base_url,
        })
    }
}

/// Email service trait for different implementations
#[async_trait::async_trait]
pub trait EmailService: Send + Sync {
    /// Send an email message
    async fn send_email(&self, message: EmailMessage) -> Result<EmailReceipt, EmailError>;

    /// Return the default "from" address for outgoing emails
    fn default_from(&self) -> String;

    /// Return the application base URL for building links
    fn app_base_url(&self) -> &str;

    /// Send team invitation email
    async fn send_team_invitation(
        &self,
        team_name: &str,
        team_id: Uuid,
        invitation_id: Uuid,
        recipient_email: &str,
        inviter_name: &str,
        role: &str,
    ) -> Result<EmailReceipt, EmailError> {
        let invitation_url = format!(
            "{}/teams/{}/invitations/{}/accept",
            self.app_base_url(),
            team_id,
            invitation_id
        );

        let subject = format!("Invitation to join team: {}", team_name);
        let body_text =
            content::team_invitation_text(inviter_name, team_name, role, &invitation_url);
        let body_html =
            content::team_invitation_html(inviter_name, team_name, role, &invitation_url);

        let message = EmailMessage::new(
            recipient_email.to_string(),
            self.default_from(),
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

/// Email service factory
pub struct EmailServiceFactory;

impl EmailServiceFactory {
    /// Create email service based on configuration
    pub async fn create(config: EmailConfig) -> Result<Box<dyn EmailService>, EmailError> {
        if !config.enabled {
            tracing::info!("Email service disabled, using mock implementation");
            return Ok(Box::new(mock::MockEmailService::new()));
        }

        match config.provider.as_str() {
            "ses" | "aws-ses" => {
                tracing::info!("Creating AWS SES email service");
                let ses_service = aws_ses::SesEmailService::new(config).await?;
                Ok(Box::new(ses_service))
            }
            "mock" => {
                tracing::info!("Creating mock email service");
                Ok(Box::new(mock::MockEmailService::new()))
            }
            provider => Err(EmailError::Configuration(format!(
                "Unknown email provider: {}. Supported providers: ses, mock",
                provider
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_message_creation() {
        let message = EmailMessage::new(
            "test@example.com".to_string(),
            "sender@example.com".to_string(),
            "Test Subject".to_string(),
            "Test body".to_string(),
        )
        .with_html("<p>Test body</p>".to_string())
        .with_reply_to("reply@example.com".to_string())
        .with_metadata("invitation_id".to_string(), "123".to_string());

        assert_eq!(message.to, "test@example.com");
        assert_eq!(message.from, "sender@example.com");
        assert_eq!(message.subject, "Test Subject");
        assert_eq!(message.body_text, "Test body");
        assert_eq!(message.body_html, Some("<p>Test body</p>".to_string()));
        assert_eq!(message.reply_to, Some("reply@example.com".to_string()));
        assert_eq!(
            message.metadata.get("invitation_id"),
            Some(&"123".to_string())
        );
    }

    #[test]
    fn test_email_config_from_env() {
        // Test with defaults
        std::env::remove_var("EMAIL_PROVIDER");
        std::env::remove_var("FROM_EMAIL");
        std::env::remove_var("EMAIL_ENABLED");

        let config = EmailConfig::from_env().unwrap();
        assert_eq!(config.provider, "mock");
        assert_eq!(config.default_from, "invitations@framecast.app");
        assert!(config.enabled);
    }
}
