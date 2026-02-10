use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use framecast_common::{Error, Result};

use crate::domain::state::{
    StateError, WebhookDeliveryEvent, WebhookDeliveryGuardContext, WebhookDeliveryState,
    WebhookDeliveryStateMachine,
};

/// Webhook event type — matches the `webhook_event_type` DB enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "webhook_event_type", rename_all = "lowercase")]
pub enum WebhookEventType {
    #[serde(rename = "job.queued")]
    #[sqlx(rename = "job.queued")]
    JobQueued,
    #[serde(rename = "job.started")]
    #[sqlx(rename = "job.started")]
    JobStarted,
    #[serde(rename = "job.progress")]
    #[sqlx(rename = "job.progress")]
    JobProgress,
    #[serde(rename = "job.completed")]
    #[sqlx(rename = "job.completed")]
    JobCompleted,
    #[serde(rename = "job.failed")]
    #[sqlx(rename = "job.failed")]
    JobFailed,
    #[serde(rename = "job.canceled")]
    #[sqlx(rename = "job.canceled")]
    JobCanceled,
}

/// Webhook entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Webhook {
    pub id: Uuid,
    pub team_id: Uuid,
    pub created_by: Uuid,
    pub url: String,
    pub events: Vec<WebhookEventType>,
    pub secret: String,
    pub is_active: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Webhook {
    /// Create a new webhook with validation
    pub fn new(
        team_id: Uuid,
        created_by: Uuid,
        url: String,
        events: Vec<WebhookEventType>,
    ) -> Result<Self> {
        // Validate URL
        if !url.starts_with("https://") {
            return Err(Error::Validation("Webhook URL must be HTTPS".to_string()));
        }

        if url.len() > 2048 {
            return Err(Error::Validation(
                "URL must be ≤2048 characters".to_string(),
            ));
        }

        // Validate events
        if events.is_empty() {
            return Err(Error::Validation(
                "Must subscribe to at least one event".to_string(),
            ));
        }

        // Generate secret for HMAC signing
        let secret = uuid::Uuid::new_v4().to_string().replace('-', "");

        let now = Utc::now();
        Ok(Webhook {
            id: Uuid::new_v4(),
            team_id,
            created_by,
            url,
            events,
            secret,
            is_active: true,
            last_triggered_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // URL validation
        if !self.url.starts_with("https://") {
            return Err(Error::Validation("Webhook URL must be HTTPS".to_string()));
        }

        if self.url.len() > 2048 {
            return Err(Error::Validation(
                "URL must be ≤2048 characters".to_string(),
            ));
        }

        // Events validation
        if self.events.is_empty() {
            return Err(Error::Validation(
                "Must subscribe to at least one event".to_string(),
            ));
        }

        Ok(())
    }
}

/// Webhook delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "webhook_delivery_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WebhookDeliveryStatus {
    #[default]
    Pending,
    Attempting,
    Delivered,
    Retrying,
    Failed,
}

impl WebhookDeliveryStatus {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> WebhookDeliveryState {
        match self {
            WebhookDeliveryStatus::Pending => WebhookDeliveryState::Pending,
            WebhookDeliveryStatus::Attempting => WebhookDeliveryState::Attempting,
            WebhookDeliveryStatus::Delivered => WebhookDeliveryState::Delivered,
            WebhookDeliveryStatus::Retrying => WebhookDeliveryState::Retrying,
            WebhookDeliveryStatus::Failed => WebhookDeliveryState::Failed,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: WebhookDeliveryState) -> Self {
        match state {
            WebhookDeliveryState::Pending => WebhookDeliveryStatus::Pending,
            WebhookDeliveryState::Attempting => WebhookDeliveryStatus::Attempting,
            WebhookDeliveryState::Delivered => WebhookDeliveryStatus::Delivered,
            WebhookDeliveryState::Retrying => WebhookDeliveryStatus::Retrying,
            WebhookDeliveryState::Failed => WebhookDeliveryStatus::Failed,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<WebhookDeliveryStatus> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| WebhookDeliveryStatus::from_state(*s))
            .collect()
    }
}

/// Webhook delivery entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub job_id: Option<Uuid>,
    pub event_type: String,
    pub status: WebhookDeliveryStatus,
    pub payload: Json<serde_json::Value>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl WebhookDelivery {
    /// Create a new webhook delivery
    pub fn new(
        webhook_id: Uuid,
        job_id: Option<Uuid>,
        event_type: String,
        payload: serde_json::Value,
    ) -> Self {
        WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id,
            job_id,
            event_type,
            status: WebhookDeliveryStatus::default(),
            payload: Json(payload),
            response_status: None,
            response_body: None,
            attempts: 0,
            max_attempts: 5,
            next_retry_at: None,
            delivered_at: None,
            created_at: Utc::now(),
        }
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Attempts validation
        if self.attempts > self.max_attempts {
            return Err(Error::Validation(
                "Attempts cannot exceed maximum".to_string(),
            ));
        }

        // Delivery validation
        if self.status == WebhookDeliveryStatus::Delivered && self.delivered_at.is_none() {
            return Err(Error::Validation(
                "Delivered webhooks must have delivery timestamp".to_string(),
            ));
        }

        Ok(())
    }

    /// Start an attempt to deliver the webhook
    pub fn start_attempt(&mut self) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::Attempt)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.attempts += 1;
        Ok(())
    }

    /// Mark delivery as successful (2xx response)
    pub fn mark_delivered(
        &mut self,
        response_status: i32,
        response_body: Option<String>,
    ) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::Success)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.response_status = Some(response_status);
        self.response_body = response_body;
        self.delivered_at = Some(Utc::now());
        self.next_retry_at = None;
        Ok(())
    }

    /// Mark for retry (5xx or timeout)
    pub fn mark_for_retry(
        &mut self,
        response_status: Option<i32>,
        response_body: Option<String>,
        next_retry_at: DateTime<Utc>,
    ) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::Retry)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.response_status = response_status;
        self.response_body = response_body;
        self.next_retry_at = Some(next_retry_at);
        Ok(())
    }

    /// Mark as permanently failed (4xx response)
    pub fn mark_failed_permanent(
        &mut self,
        response_status: i32,
        response_body: Option<String>,
    ) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::PermanentFailure)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.response_status = Some(response_status);
        self.response_body = response_body;
        self.next_retry_at = None;
        Ok(())
    }

    /// Mark as failed due to max attempts exceeded
    pub fn mark_failed_max_attempts(&mut self) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::MaxAttemptsExceeded)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.next_retry_at = None;
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: WebhookDeliveryEvent) -> Result<WebhookDeliveryState> {
        let current_state = self.status.to_state();
        let context = WebhookDeliveryGuardContext {
            attempt_count: self.attempts as u32,
            max_attempts: self.max_attempts as u32,
        };
        WebhookDeliveryStateMachine::transition(current_state, event, Some(&context)).map_err(|e| {
            match e {
                StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                    "Invalid webhook delivery transition: cannot apply '{}' event from '{}' state",
                    event, from
                )),
                StateError::TerminalState(state) => Error::Validation(format!(
                    "Webhook delivery is in terminal state '{}' and cannot transition",
                    state
                )),
                StateError::GuardFailed(msg) => Error::Validation(msg),
            }
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &WebhookDeliveryEvent) -> bool {
        let context = WebhookDeliveryGuardContext {
            attempt_count: self.attempts as u32,
            max_attempts: self.max_attempts as u32,
        };
        WebhookDeliveryStateMachine::can_transition(self.status.to_state(), event, Some(&context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_webhook_creation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let url = "https://example.com/webhook".to_string();
        let events = vec![WebhookEventType::JobCompleted, WebhookEventType::JobFailed];

        let webhook = Webhook::new(team_id, created_by, url.clone(), events.clone()).unwrap();

        assert_eq!(webhook.team_id, team_id);
        assert_eq!(webhook.created_by, created_by);
        assert_eq!(webhook.url, url);
        assert_eq!(webhook.events, events);
        assert!(!webhook.secret.is_empty());
        assert!(webhook.is_active);
    }

    #[test]
    fn test_webhook_validation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();

        // Test non-HTTPS URL
        let result = Webhook::new(
            team_id,
            created_by,
            "http://example.com/webhook".to_string(),
            vec![WebhookEventType::JobCompleted],
        );
        assert!(result.is_err());

        // Test empty events
        let result = Webhook::new(
            team_id,
            created_by,
            "https://example.com/webhook".to_string(),
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_webhook_delivery_creation() {
        let webhook_id = Uuid::new_v4();
        let job_id = Some(Uuid::new_v4());
        let event_type = "job.completed".to_string();
        let payload = json!({"job_id": job_id, "status": "completed"});

        let delivery =
            WebhookDelivery::new(webhook_id, job_id, event_type.clone(), payload.clone());

        assert_eq!(delivery.webhook_id, webhook_id);
        assert_eq!(delivery.job_id, job_id);
        assert_eq!(delivery.event_type, event_type);
        assert_eq!(delivery.payload.0, payload);
        assert_eq!(delivery.status, WebhookDeliveryStatus::Pending);
        assert_eq!(delivery.attempts, 0);
        assert_eq!(delivery.max_attempts, 5);
    }
}
