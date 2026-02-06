//! LocalStack HTTP Client for Integration Tests
//!
//! Provides functionality to retrieve and parse emails sent through LocalStack SES
//! for comprehensive integration testing of email workflows.

use std::time::Duration;

use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

/// Raw LocalStack SES message format (matches `/_aws/ses` response)
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct RawLocalStackMessage {
    #[serde(alias = "Id")]
    id: String,
    #[serde(alias = "Region")]
    region: Option<String>,
    #[serde(alias = "Source")]
    source: Option<String>,
    #[serde(alias = "Destination")]
    destination: Option<RawDestination>,
    #[serde(alias = "Subject")]
    subject: Option<String>,
    #[serde(alias = "Body")]
    body: Option<RawBody>,
    #[serde(alias = "Timestamp")]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct RawDestination {
    #[serde(alias = "ToAddresses", default)]
    to_addresses: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct RawBody {
    text_part: Option<String>,
    html_part: Option<String>,
}

/// Represents an email retrieved from LocalStack SES API (normalized)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LocalStackEmail {
    pub id: String,
    pub subject: String,
    pub body: String,
    pub from: String,
    pub to: Vec<String>,
    pub timestamp: Option<String>,
}

/// Response wrapper for LocalStack `/_aws/ses` endpoint
#[derive(Debug, Deserialize)]
struct SesMessagesResponse {
    messages: Vec<RawLocalStackMessage>,
}

impl From<RawLocalStackMessage> for LocalStackEmail {
    fn from(raw: RawLocalStackMessage) -> Self {
        let body = raw
            .body
            .map(|b| {
                // Prefer HTML part, fall back to text part
                b.html_part
                    .or(b.text_part)
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let to = raw
            .destination
            .map(|d| d.to_addresses)
            .unwrap_or_default();

        Self {
            id: raw.id,
            subject: raw.subject.unwrap_or_default(),
            body,
            from: raw.source.unwrap_or_default(),
            to,
            timestamp: raw.timestamp,
        }
    }
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

#[allow(dead_code)] // Methods available for future use and testing
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

    /// Create a LocalStack client from environment variables
    /// Reads `AWS_ENDPOINT_URL` with fallback to `http://localhost:4566`
    pub fn from_env() -> Self {
        let endpoint = std::env::var("AWS_ENDPOINT_URL")
            .unwrap_or_else(|_| "http://localhost:4566".to_string());
        Self::new(&endpoint)
    }

    /// Get all emails sent to a specific recipient address.
    ///
    /// LocalStack's `/_aws/ses` `email` query parameter filters by **sender**,
    /// so we fetch all messages and filter by destination on the client side.
    pub async fn get_emails(&self, recipient: &str) -> Result<Vec<LocalStackEmail>> {
        let all = self.get_all_emails().await?;
        let filtered = all
            .into_iter()
            .filter(|e| e.to.iter().any(|addr| addr == recipient))
            .collect();
        Ok(filtered)
    }

    /// Fetch every email stored in LocalStack SES (no filters).
    async fn get_all_emails(&self) -> Result<Vec<LocalStackEmail>> {
        let url = format!("{}/_aws/ses", self.base_url);

        let response = self
            .client
            .get(&url)
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

        let data: SesMessagesResponse = response.json().await?;
        let emails = data.messages.into_iter().map(LocalStackEmail::from).collect();
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

        let invitation_emails: Vec<LocalStackEmail> =
            emails.into_iter().filter(|e| e.is_invitation()).collect();

        if invitation_emails.is_empty() {
            return Ok(None);
        }

        // Sort by timestamp and return most recent
        let mut sorted_emails = invitation_emails;
        sorted_emails.sort_by(|a, b| match (&a.timestamp, &b.timestamp) {
            (Some(ts_a), Some(ts_b)) => ts_b.cmp(ts_a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.id.cmp(&a.id),
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
                            return Some(format!(
                                "https://framecast.app/{}",
                                url.trim_start_matches('/')
                            ));
                        }
                    }
                }
            }
        }

        None
    }

    /// Wait for an email to arrive (with polling)
    #[allow(dead_code)] // Available for future test scenarios
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

    fn make_email(id: &str, subject: &str, body: &str) -> LocalStackEmail {
        LocalStackEmail {
            id: id.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            from: "noreply@framecast.app".to_string(),
            to: vec!["user@example.com".to_string()],
            timestamp: None,
        }
    }

    #[test]
    fn test_email_is_invitation() {
        let invitation_email = make_email("test", "You're invited to join Test Team", "Click here to accept");
        assert!(invitation_email.is_invitation());

        let regular_email = make_email("test2", "Welcome to Framecast", "Welcome message");
        assert!(!regular_email.is_invitation());
    }

    #[test]
    fn test_extract_invitation_id() {
        let client = LocalStackEmailClient::localhost();

        let email = make_email(
            "test",
            "Invitation",
            "Accept invitation: https://framecast.app/teams/team123/invitations/12345678-1234-4567-89ab-123456789012/accept", // pragma: allowlist secret
        );

        let invitation_id = client.extract_invitation_id(&email);
        assert!(invitation_id.is_some());
        assert_eq!(
            invitation_id.unwrap().to_string(),
            "12345678-1234-4567-89ab-123456789012" // pragma: allowlist secret
        );
    }

    #[test]
    fn test_extract_team_id() {
        let client = LocalStackEmailClient::localhost();

        let email = make_email(
            "test",
            "Team Invitation",
            "Join team: https://framecast.app/teams/87654321-4321-4654-ba98-876543210987/invitations/12345678-1234-4567-89ab-123456789012/accept", // pragma: allowlist secret
        );

        let team_id = client.extract_team_id(&email);
        assert!(team_id.is_some());
        assert_eq!(
            team_id.unwrap().to_string(),
            "87654321-4321-4654-ba98-876543210987" // pragma: allowlist secret
        );
    }

    #[test]
    fn test_extract_invitation_url() {
        let client = LocalStackEmailClient::localhost();

        let email = make_email(
            "test",
            "Invitation",
            r#"<a href="https://framecast.app/teams/team123/invitations/12345678-1234-4567-89ab-123456789012/accept">Accept Invitation</a>"#, // pragma: allowlist secret
        );

        let url = client.extract_invitation_url(&email);
        assert!(url.is_some());
        let url_str = url.unwrap();
        assert!(url_str.contains("invitations") && url_str.contains("accept"));
    }
}
