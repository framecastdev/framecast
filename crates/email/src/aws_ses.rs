//! AWS SES Email Service Implementation
//!
//! Provides production email delivery through AWS Simple Email Service (SES)
//! with support for LocalStack testing environment.

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_ses::config::SharedCredentialsProvider;
use aws_sdk_ses::types::{Body, Content, Destination, Message};
use aws_sdk_ses::Client as SesClient;
use chrono::Utc;

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

    fn default_from(&self) -> String {
        self.config.default_from.clone()
    }

    fn app_base_url(&self) -> &str {
        &self.config.app_base_url
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
            enabled: true,
            app_base_url: "https://framecast.app".to_string(),
        };

        // This will work with LocalStack but may fail with real AWS without credentials
        // In real testing, we'd configure proper test credentials
        let result = SesEmailService::new(config).await;

        // We expect this to succeed in creating the service, even if health check fails
        assert!(result.is_ok());
    }
}
