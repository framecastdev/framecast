//! LocalStack HTTP Client for Integration Tests
//!
//! Provides functionality to retrieve and parse emails sent through LocalStack SES
//! for comprehensive integration testing of email workflows.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

/// Represents an email retrieved from LocalStack SES API
#[derive(Debug, Deserialize, Clone)]
pub struct LocalStackEmail {
    #[serde(default = "default_id")]
    pub id: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    pub timestamp: Option<String>,

    #[serde(flatten)]
    #[allow(dead_code)]  // Reserved for debugging and future extensibility
    pub raw_data: HashMap<String, serde_json::Value>,
}

fn default_id() -> String {
    format!("email_{}", uuid::Uuid::new_v4().simple())
}

impl LocalStackEmail {
    /// Check if this email is an invitation email
    pub fn is_invitation(&self) -> bool {
        self.subject.to_lowercase().contains("invitation")
            || self.subject.to_lowercase().contains("invite")
    }
}

/// LocalStack HTTP client for email retrieval
#[derive(Debug)]
pub struct LocalStackEmailClient {
    base_url: String,
    client: Client,
}

impl LocalStackEmailClient {
    /// Create a new LocalStack email client
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }

    /// Create a default LocalStack client for localhost
    pub fn localhost() -> Self {
        Self::new("http://localhost:4566")
    }

    /// Get all emails for a specific email address
    pub async fn get_emails(&self, email: &str) -> Result<Vec<LocalStackEmail>> {
        let url = format!("{}/_aws/ses", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("email", email)])
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "LocalStack SES API returned error: {} - {}",
                response.status(),
                response.text().await?
            );
        }

        let data: serde_json::Value = response.json().await?;

        // Handle different response formats from LocalStack
        let emails: Vec<LocalStackEmail> = match data {
            serde_json::Value::Array(emails) => {
                // Direct array of emails
                let mut result = Vec::new();
                for email in emails {
                    if let Ok(parsed) = serde_json::from_value::<LocalStackEmail>(email) {
                        result.push(parsed);
                    }
                }
                result
            },
            serde_json::Value::Object(obj) if obj.contains_key("emails") => {
                // Wrapped in "emails" field
                if let Ok(emails) = serde_json::from_value::<Vec<LocalStackEmail>>(obj["emails"].clone()) {
                    emails
                } else {
                    Vec::new()
                }
            },
            serde_json::Value::Object(obj) if obj.contains_key("messages") => {
                // LocalStack format: wrapped in "messages" field
                if let Ok(emails) = serde_json::from_value::<Vec<LocalStackEmail>>(obj["messages"].clone()) {
                    emails
                } else {
                    Vec::new()
                }
            },
            serde_json::Value::Object(obj) => {
                // Single email object - try to parse it directly
                if let Ok(email) = serde_json::from_value::<LocalStackEmail>(serde_json::Value::Object(obj)) {
                    vec![email]
                } else {
                    Vec::new()
                }
            },
            _ => Vec::new(),
        };

        Ok(emails)
    }

    /// Get the most recent email for an email address
    pub async fn get_latest_email(&self, email: &str) -> Result<Option<LocalStackEmail>> {
        let emails = self.get_emails(email).await?;

        if emails.is_empty() {
            return Ok(None);
        }

        // Sort by timestamp if available, otherwise by ID
        let mut sorted_emails = emails;
        sorted_emails.sort_by(|a, b| {
            match (&a.timestamp, &b.timestamp) {
                (Some(ts_a), Some(ts_b)) => ts_b.cmp(ts_a), // Most recent first
                (Some(_), None) => std::cmp::Ordering::Less, // Timestamped emails first
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.id.cmp(&a.id), // Fall back to ID comparison
            }
        });

        Ok(sorted_emails.into_iter().next())
    }

    /// Get the most recent invitation email for an email address
    pub async fn get_latest_invitation(&self, email: &str) -> Result<Option<LocalStackEmail>> {
        let emails = self.get_emails(email).await?;

        let invitation_emails: Vec<LocalStackEmail> = emails
            .into_iter()
            .filter(|e| e.is_invitation())
            .collect();

        if invitation_emails.is_empty() {
            return Ok(None);
        }

        // Sort by timestamp and return most recent
        let mut sorted_emails = invitation_emails;
        sorted_emails.sort_by(|a, b| {
            match (&a.timestamp, &b.timestamp) {
                (Some(ts_a), Some(ts_b)) => ts_b.cmp(ts_a),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.id.cmp(&a.id),
            }
        });

        Ok(sorted_emails.into_iter().next())
    }

    /// Delete a specific email by message ID
    pub async fn delete_email(&self, message_id: &str) -> Result<bool> {
        let url = format!("{}/_aws/ses", self.base_url);

        let response = self
            .client
            .delete(&url)
            .query(&[("id", message_id)])
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    /// Clear all emails for a specific email address
    pub async fn clear_emails(&self, email: &str) -> Result<usize> {
        let emails = self.get_emails(email).await?;
        let mut deleted_count = 0;

        for email_obj in emails {
            if self.delete_email(&email_obj.id).await.unwrap_or(false) {
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }

    /// Extract invitation ID (UUID) from email content
    pub fn extract_invitation_id(&self, email: &LocalStackEmail) -> Option<Uuid> {
        let content = format!("{} {}", email.subject, email.body);

        // UUID v4 pattern (simplified)
        let uuid_pattern = r"([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})";

        // Try to find UUID in different contexts
        let patterns = vec![
            // In invitation URLs
            format!(r"/invitations/{}/accept", uuid_pattern),
            // In any URL context
            format!(r"invitations/{}", uuid_pattern),
            // As parameter
            format!(r"invitation_id={}", uuid_pattern),
            format!(r"id={}", uuid_pattern),
        ];

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(captures) = re.captures(&content) {
                    if let Some(uuid_match) = captures.get(1) {
                        if let Ok(uuid) = Uuid::parse_str(uuid_match.as_str()) {
                            return Some(uuid);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract team ID (UUID) from email content
    pub fn extract_team_id(&self, email: &LocalStackEmail) -> Option<Uuid> {
        let content = format!("{} {}", email.subject, email.body);

        // UUID v4 pattern (simplified)
        let uuid_pattern = r"([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})";

        // Try to find UUID in different contexts
        let patterns = vec![
            // In team URLs
            format!(r"/teams/{}/", uuid_pattern),
            format!(r"/teams/{}/invitations", uuid_pattern),
            // As parameter
            format!(r"team_id={}", uuid_pattern),
        ];

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(captures) = re.captures(&content) {
                    if let Some(uuid_match) = captures.get(1) {
                        if let Ok(uuid) = Uuid::parse_str(uuid_match.as_str()) {
                            return Some(uuid);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract invitation URL from email content
    pub fn extract_invitation_url(&self, email: &LocalStackEmail) -> Option<String> {
        let content = &email.body;

        // Simple patterns without complex escaping
        let patterns = vec![
            // Full URL patterns
            r"(https?://[^\s]+/teams/[^\s]+/invitations/[^\s]+/accept)",
            r"(https?://framecast\.app/teams/[^\s]+/invitations/[^\s]+/accept)",
            // Relative URL patterns
            r"(/teams/[^\s]+/invitations/[^\s]+/accept)",
        ];

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(captures) = re.captures(content) {
                    if let Some(url_match) = captures.get(1) {
                        let url = url_match.as_str();

                        // Make relative URLs absolute
                        if url.starts_with('/') {
                            return Some(format!("https://framecast.app{}", url));
                        } else if url.starts_with("http") {
                            return Some(url.to_string());
                        } else {
                            return Some(format!("https://framecast.app/{}", url.trim_start_matches('/')));
                        }
                    }
                }
            }
        }

        None
    }

    /// Wait for an email to arrive (with polling)
    #[allow(dead_code)]  // Available for future test scenarios
    pub async fn wait_for_email(
        &self,
        email: &str,
        timeout_secs: u64,
    ) -> Result<Option<LocalStackEmail>> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            if let Some(email_obj) = self.get_latest_email(email).await? {
                return Ok(Some(email_obj));
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(None)
    }

    /// Wait for an invitation email to arrive (with polling)
    pub async fn wait_for_invitation_email(
        &self,
        email: &str,
        timeout_secs: u64,
    ) -> Result<Option<LocalStackEmail>> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            if let Some(email_obj) = self.get_latest_invitation(email).await? {
                return Ok(Some(email_obj));
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(None)
    }

    /// Check if LocalStack SES service is available
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/_localstack/health", self.base_url);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        if response.status().is_success() {
            let health: serde_json::Value = response.json().await?;
            if let Some(services) = health.get("services") {
                if let Some(ses_status) = services.get("ses") {
                    return Ok(ses_status == "available" || ses_status == "running");
                }
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_is_invitation() {
        let invitation_email = LocalStackEmail {
            id: "test".to_string(),
            subject: "You're invited to join Test Team".to_string(),
            body: "Click here to accept".to_string(),
            from: "noreply@framecast.app".to_string(),
            to: vec!["user@example.com".to_string()],
            timestamp: None,
            raw_data: HashMap::new(),
        };

        assert!(invitation_email.is_invitation());

        let regular_email = LocalStackEmail {
            id: "test2".to_string(),
            subject: "Welcome to Framecast".to_string(),
            body: "Welcome message".to_string(),
            from: "noreply@framecast.app".to_string(),
            to: vec!["user@example.com".to_string()],
            timestamp: None,
            raw_data: HashMap::new(),
        };

        assert!(!regular_email.is_invitation());
    }

    #[test]
    fn test_extract_invitation_id() {
        let client = LocalStackEmailClient::localhost();

        let email = LocalStackEmail {
            id: "test".to_string(),
            subject: "Invitation".to_string(),
            body: "Accept invitation: https://framecast.app/teams/team123/invitations/12345678-1234-4567-89ab-123456789012/accept".to_string(),
            from: "noreply@framecast.app".to_string(),
            to: vec!["user@example.com".to_string()],
            timestamp: None,
            raw_data: HashMap::new(),
        };

        let invitation_id = client.extract_invitation_id(&email);
        assert!(invitation_id.is_some());
        assert_eq!(invitation_id.unwrap().to_string(), "12345678-1234-4567-89ab-123456789012");
    }

    #[test]
    fn test_extract_team_id() {
        let client = LocalStackEmailClient::localhost();

        let email = LocalStackEmail {
            id: "test".to_string(),
            subject: "Team Invitation".to_string(),
            body: "Join team: https://framecast.app/teams/87654321-4321-7654-ba98-876543210987/invitations/12345678-1234-4567-89ab-123456789012/accept".to_string(),
            from: "noreply@framecast.app".to_string(),
            to: vec!["user@example.com".to_string()],
            timestamp: None,
            raw_data: HashMap::new(),
        };

        let team_id = client.extract_team_id(&email);
        assert!(team_id.is_some());
        assert_eq!(team_id.unwrap().to_string(), "87654321-4321-7654-ba98-876543210987");
    }

    #[test]
    fn test_extract_invitation_url() {
        let client = LocalStackEmailClient::localhost();

        let email = LocalStackEmail {
            id: "test".to_string(),
            subject: "Invitation".to_string(),
            body: r#"<a href="https://framecast.app/teams/team123/invitations/12345678-1234-4567-89ab-123456789012/accept">Accept Invitation</a>"#.to_string(),
            from: "noreply@framecast.app".to_string(),
            to: vec!["user@example.com".to_string()],
            timestamp: None,
            raw_data: HashMap::new(),
        };

        let url = client.extract_invitation_url(&email);
        assert!(url.is_some());
        let url_str = url.unwrap();
        assert!(url_str.contains("invitations") && url_str.contains("accept"));
    }
}