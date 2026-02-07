//! Job domain entities for Framecast
//!
//! This module contains job-related domain entities as defined in the API specification.
//! Each entity includes proper validation, serialization, and business rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use framecast_common::{Error, Result, Urn};

use crate::domain::state::{JobEvent, JobState, JobStateMachine, StateError};

/// Job status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "job_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    #[default]
    Queued,
    Processing,
    Completed,
    Failed,
    Canceled,
}

impl JobStatus {
    /// Check if status is terminal (job has finished)
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> JobState {
        match self {
            JobStatus::Queued => JobState::Queued,
            JobStatus::Processing => JobState::Processing,
            JobStatus::Completed => JobState::Completed,
            JobStatus::Failed => JobState::Failed,
            JobStatus::Canceled => JobState::Canceled,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: JobState) -> Self {
        match state {
            JobState::Queued => JobStatus::Queued,
            JobState::Processing => JobStatus::Processing,
            JobState::Completed => JobStatus::Completed,
            JobState::Failed => JobStatus::Failed,
            JobState::Canceled => JobStatus::Canceled,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<JobStatus> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| JobStatus::from_state(*s))
            .collect()
    }
}

/// Job failure type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "job_failure_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum JobFailureType {
    System,
    Validation,
    Timeout,
    Canceled,
}

/// Job entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub owner: String, // URN as string
    pub triggered_by: Uuid,
    pub project_id: Option<Uuid>,
    pub status: JobStatus,
    pub spec_snapshot: Json<serde_json::Value>,
    pub options: Json<serde_json::Value>,
    pub progress: Json<serde_json::Value>,
    pub output: Option<Json<serde_json::Value>>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<Json<serde_json::Value>>,
    pub credits_charged: i32,
    pub failure_type: Option<JobFailureType>,
    pub credits_refunded: i32,
    pub idempotency_key: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    /// Create a new job with validation
    pub fn new(
        owner: Urn,
        triggered_by: Uuid,
        project_id: Option<Uuid>,
        spec_snapshot: serde_json::Value,
        credits_charged: i32,
        idempotency_key: Option<String>,
    ) -> Result<Self> {
        // Validate credits
        if credits_charged < 0 {
            return Err(Error::Validation(
                "Credits charged cannot be negative".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Job {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            triggered_by,
            project_id,
            status: JobStatus::default(),
            spec_snapshot: Json(spec_snapshot),
            options: Json(serde_json::Value::Object(serde_json::Map::new())),
            progress: Json(serde_json::Value::Object(serde_json::Map::new())),
            output: None,
            output_size_bytes: None,
            error: None,
            credits_charged,
            failure_type: None,
            credits_refunded: 0,
            idempotency_key,
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Check if job is ephemeral (not tied to project)
    pub fn is_ephemeral(&self) -> bool {
        self.project_id.is_none()
    }

    /// Check if job is terminal
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Get net credits (charged - refunded)
    pub fn net_credits(&self) -> i32 {
        self.credits_charged - self.credits_refunded
    }

    /// Start job processing
    pub fn start(&mut self) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::WorkerPicksUp)?;
        self.status = JobStatus::from_state(new_state);
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Complete job successfully
    pub fn complete(
        &mut self,
        output: serde_json::Value,
        output_size_bytes: Option<i64>,
    ) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::Success)?;
        self.status = JobStatus::from_state(new_state);
        self.output = Some(Json(output));
        self.output_size_bytes = output_size_bytes;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Fail job
    pub fn fail(&mut self, error: serde_json::Value, failure_type: JobFailureType) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::Failure)?;
        self.status = JobStatus::from_state(new_state);
        self.error = Some(Json(error));
        self.failure_type = Some(failure_type.clone());

        // Apply refund based on failure type and progress
        self.apply_refund(failure_type);

        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Cancel job
    pub fn cancel(&mut self) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::Cancel)?;
        self.status = JobStatus::from_state(new_state);
        self.failure_type = Some(JobFailureType::Canceled);

        // Apply refund with 10% cancellation fee
        self.apply_refund(JobFailureType::Canceled);

        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: JobEvent) -> Result<JobState> {
        let current_state = self.status.to_state();
        JobStateMachine::transition(current_state, event).map_err(|e| match e {
            StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                "Invalid job transition: cannot apply '{}' event from '{}' state",
                event, from
            )),
            StateError::TerminalState(state) => Error::Validation(format!(
                "Job is in terminal state '{}' and cannot transition",
                state
            )),
            StateError::GuardFailed(msg) => Error::Validation(msg),
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &JobEvent) -> bool {
        JobStateMachine::can_transition(self.status.to_state(), event)
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Calculate refund amount based on failure type and progress
    pub fn calculate_refund(&self, failure_type: JobFailureType) -> i32 {
        let progress_percent_raw = self.get_progress_percent();

        // Convert to integer with 2 decimal precision (10000 = 100.00%)
        let progress_int = (progress_percent_raw * 100.0).round() as i32;
        let progress_int = progress_int.clamp(0, 10000); // 0.00% to 100.00%

        match failure_type {
            // Full refund for system errors and timeouts
            JobFailureType::System | JobFailureType::Timeout => self.credits_charged,

            // Partial refund based on remaining work for validation errors
            JobFailureType::Validation => {
                let remaining_work_int = 10000 - progress_int; // Remaining work as integer
                                                               // FLOOR operation: integer division automatically floors for positive numbers
                                                               // Use i64 for intermediate calculation to prevent overflow
                let result = (self.credits_charged as i64 * remaining_work_int as i64) / 10000;
                result as i32 // Safe because result will be <= self.credits_charged (which fits in i32)
            }

            // Partial refund with 10% cancellation fee
            JobFailureType::Canceled => {
                let remaining_work_int = 10000 - progress_int;

                // Calculate: credits * remaining_work * 0.9 using i64 to prevent overflow
                // = (credits * remaining_work * 9000) / (10000 * 10000)
                let refund_before_cap =
                    (self.credits_charged as i64 * remaining_work_int as i64 * 9000) / 100_000_000; // 10000 * 10000

                // Enforce minimum 10% charge (maximum 90% refund) - SPEC REQUIREMENT
                let max_refund = (self.credits_charged as i64 * 9000) / 10000; // 90% of charged amount

                std::cmp::min(refund_before_cap as i32, max_refund as i32)
            }
        }
    }

    /// Get progress percentage from progress field
    pub fn get_progress_percent(&self) -> f64 {
        let raw_progress = self
            .progress
            .0
            .get("percent")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Round to 2 decimal places to avoid precision issues
        let rounded = (raw_progress * 100.0).round() / 100.0;
        rounded.clamp(0.0, 100.0)
    }

    /// Apply refund to the job based on failure type
    pub fn apply_refund(&mut self, failure_type: JobFailureType) {
        self.credits_refunded = self.calculate_refund(failure_type);
    }

    /// Update progress percentage
    pub fn update_progress(&mut self, percent: f64) -> Result<()> {
        if !(0.0..=100.0).contains(&percent) {
            return Err(Error::Validation(
                "Progress must be between 0 and 100".to_string(),
            ));
        }

        if let Some(progress_obj) = self.progress.0.as_object_mut() {
            progress_obj.insert(
                "percent".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(percent)
                        .ok_or_else(|| Error::Validation("Invalid progress value".to_string()))?,
                ),
            );
        } else {
            // Create new progress object
            let mut progress_map = serde_json::Map::new();
            progress_map.insert(
                "percent".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(percent)
                        .ok_or_else(|| Error::Validation("Invalid progress value".to_string()))?,
                ),
            );
            self.progress = Json(serde_json::Value::Object(progress_map));
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // INV-J8: Cannot refund more than charged
        if self.credits_refunded > self.credits_charged {
            return Err(Error::Validation(
                "Cannot refund more than charged".to_string(),
            ));
        }

        // INV-J9: Credits values cannot be negative
        if self.credits_refunded < 0 || self.credits_charged < 0 {
            return Err(Error::Validation(
                "Credits values cannot be negative".to_string(),
            ));
        }

        // INV-J2: Terminal jobs have completion timestamp
        if self.is_terminal() && self.completed_at.is_none() {
            return Err(Error::Validation(
                "Terminal jobs must have completion timestamp".to_string(),
            ));
        }

        // INV-J3: Processing jobs have start timestamp
        if self.status == JobStatus::Processing && self.started_at.is_none() {
            return Err(Error::Validation(
                "Processing jobs must have start timestamp".to_string(),
            ));
        }

        // INV-J4: Completed jobs must have output
        if self.status == JobStatus::Completed && self.output.is_none() {
            return Err(Error::Validation(
                "Completed jobs must have output".to_string(),
            ));
        }

        // INV-J5: Failed jobs must have error
        if self.status == JobStatus::Failed && self.error.is_none() {
            return Err(Error::Validation("Failed jobs must have error".to_string()));
        }

        // INV-J6 & J7: Failure type consistency
        match (&self.status, &self.failure_type) {
            (JobStatus::Failed | JobStatus::Canceled, None) => {
                return Err(Error::Validation(
                    "Failed/canceled jobs must have failure type".to_string(),
                ));
            }
            (JobStatus::Completed, Some(_)) => {
                return Err(Error::Validation(
                    "Completed jobs must not have failure type".to_string(),
                ));
            }
            _ => {}
        }

        // INV-J11: Project jobs must be team-owned
        if self.project_id.is_some() {
            let urn = self.owner_urn()?;
            if !urn.is_team() {
                return Err(Error::Validation(
                    "Project-based jobs must be team-owned".to_string(),
                ));
            }
        }

        // SPEC: Cancellation must charge at least 10% (maximum 90% refund)
        if let Some(JobFailureType::Canceled) = self.failure_type {
            let min_charge = (self.credits_charged * 10) / 100; // 10% minimum
            let actual_charge = self.credits_charged - self.credits_refunded;
            if actual_charge < min_charge {
                return Err(Error::Validation(
                    "Cancellation must charge at least 10%".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Usage entity for billing metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Usage {
    pub id: Uuid,
    pub owner: String,  // URN as string
    pub period: String, // Format: YYYY-MM
    pub renders_count: i32,
    pub render_seconds: i32,
    pub credits_used: i32,
    pub credits_refunded: i32,
    pub api_calls: i32,
    pub updated_at: DateTime<Utc>,
}

impl Usage {
    /// Create new usage record
    pub fn new(owner: Urn, period: String) -> Result<Self> {
        // Validate period format (YYYY-MM)
        if period.len() != 7 {
            return Err(Error::Validation(
                "Period must be YYYY-MM format".to_string(),
            ));
        }

        let regex = regex::Regex::new(r"^\d{4}-(0[1-9]|1[0-2])$").unwrap();
        if !regex.is_match(&period) {
            return Err(Error::Validation(
                "Period must be YYYY-MM format".to_string(),
            ));
        }

        Ok(Usage {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            period,
            renders_count: 0,
            render_seconds: 0,
            credits_used: 0,
            credits_refunded: 0,
            api_calls: 0,
            updated_at: Utc::now(),
        })
    }

    /// Get net credits (used - refunded)
    pub fn net_credits(&self) -> i32 {
        self.credits_used - self.credits_refunded
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Period format validation - check length first, then regex
        if self.period.len() != 7 {
            return Err(Error::Validation(
                "Period format must be YYYY-MM".to_string(),
            ));
        }

        let regex = regex::Regex::new(r"^\d{4}-(0[1-9]|1[0-2])$").unwrap();
        if !regex.is_match(&self.period) {
            return Err(Error::Validation(
                "Period format must be YYYY-MM".to_string(),
            ));
        }

        // Counts cannot be negative
        if self.renders_count < 0 || self.credits_used < 0 || self.api_calls < 0 {
            return Err(Error::Validation(
                "Usage counts cannot be negative".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_job_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();
        let spec = json!({"type": "render", "duration": 30});
        let credits_charged = 100;

        let job = Job::new(
            owner.clone(),
            triggered_by,
            None,
            spec.clone(),
            credits_charged,
            None,
        )
        .unwrap();

        assert_eq!(job.owner_urn().unwrap(), owner);
        assert_eq!(job.triggered_by, triggered_by);
        assert!(job.is_ephemeral());
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.credits_charged, credits_charged);
        assert_eq!(job.credits_refunded, 0);
        assert!(!job.is_terminal());
    }

    #[test]
    fn test_job_state_transitions() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();
        let mut job = Job::new(owner, triggered_by, None, json!({}), 100, None).unwrap();

        // Start job
        job.start().unwrap();
        assert_eq!(job.status, JobStatus::Processing);
        assert!(job.started_at.is_some());

        // Complete job
        let output = json!({"url": "https://example.com/video.mp4"});
        job.complete(output.clone(), Some(1024)).unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.output.is_some());
        assert_eq!(job.output_size_bytes, Some(1024));
        assert!(job.is_terminal());
    }

    #[test]
    fn test_job_failure() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();
        let mut job = Job::new(owner, triggered_by, None, json!({}), 100, None).unwrap();

        job.start().unwrap();

        let error = json!({"message": "Rendering failed", "code": "RENDER_ERROR"});
        job.fail(error.clone(), JobFailureType::System).unwrap();

        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.is_some());
        assert_eq!(job.failure_type, Some(JobFailureType::System));
        assert!(job.is_terminal());
    }

    #[test]
    fn test_job_invariants() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();

        // Test negative credits
        let result = Job::new(owner.clone(), triggered_by, None, json!({}), -1, None);
        assert!(result.is_err());

        let mut job = Job::new(owner, triggered_by, None, json!({}), 100, None).unwrap();

        // Valid job
        assert!(job.validate().is_ok());

        // Invalid: refund more than charged
        job.credits_refunded = 150;
        assert!(job.validate().is_err());
    }

    #[test]
    fn test_job_project_team_constraint() {
        let team_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let team_owner = Urn::team(team_id);
        let user_owner = Urn::user(user_id);

        // Project job must be team-owned
        let project_job = Job::new(
            team_owner,
            user_id,
            Some(Uuid::new_v4()), // project_id
            json!({}),
            100,
            None,
        )
        .unwrap();
        assert!(project_job.validate().is_ok());

        // Project job cannot be user-owned
        let invalid_job = Job::new(
            user_owner,
            user_id,
            Some(Uuid::new_v4()), // project_id
            json!({}),
            100,
            None,
        )
        .unwrap();
        assert!(invalid_job.validate().is_err());
    }

    #[test]
    fn test_job_status_terminal() {
        assert!(!JobStatus::Queued.is_terminal());
        assert!(!JobStatus::Processing.is_terminal());
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Canceled.is_terminal());
    }

    #[test]
    fn test_job_refund_calculation() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Set 40% progress
        job.update_progress(40.0).unwrap();

        // System error: Full refund
        let system_refund = job.calculate_refund(JobFailureType::System);
        assert_eq!(system_refund, 100);

        // Timeout: Full refund
        let timeout_refund = job.calculate_refund(JobFailureType::Timeout);
        assert_eq!(timeout_refund, 100);

        // Validation error: Partial refund based on remaining work
        // 60% remaining = 60 credits refunded
        let validation_refund = job.calculate_refund(JobFailureType::Validation);
        assert_eq!(validation_refund, 60);

        // Cancellation: Partial refund with 10% fee
        // 60% remaining x 0.9 = 54 credits refunded
        let cancel_refund = job.calculate_refund(JobFailureType::Canceled);
        assert_eq!(cancel_refund, 54);
    }

    #[test]
    fn test_job_progress_methods() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Initially 0% progress
        assert_eq!(job.get_progress_percent(), 0.0);

        // Update progress
        job.update_progress(25.5).unwrap();
        assert_eq!(job.get_progress_percent(), 25.5);

        // Progress bounds validation
        assert!(job.update_progress(-1.0).is_err());
        assert!(job.update_progress(101.0).is_err());

        // Progress clamped to bounds in getter
        job.progress = Json(json!({"percent": 150.0}));
        assert_eq!(job.get_progress_percent(), 100.0);

        job.progress = Json(json!({"percent": -50.0}));
        assert_eq!(job.get_progress_percent(), 0.0);
    }

    #[test]
    fn test_job_fail_with_automatic_refund() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Start the job
        job.start().unwrap();
        assert_eq!(job.status, JobStatus::Processing);

        // Set some progress
        job.update_progress(30.0).unwrap();

        // Fail with system error
        job.fail(json!({"error": "GPU crashed"}), JobFailureType::System)
            .unwrap();

        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.failure_type, Some(JobFailureType::System));
        assert_eq!(job.credits_refunded, 100); // Full refund for system error
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_job_cancel_with_automatic_refund() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Start the job
        job.start().unwrap();
        assert_eq!(job.status, JobStatus::Processing);

        // Set some progress (20%)
        job.update_progress(20.0).unwrap();

        // Cancel the job
        job.cancel().unwrap();

        assert_eq!(job.status, JobStatus::Canceled);
        assert_eq!(job.failure_type, Some(JobFailureType::Canceled));

        // 80% remaining work x 0.9 (10% cancellation fee) = 72 credits refunded
        assert_eq!(job.credits_refunded, 72);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_job_refund_edge_cases() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // 0% progress - full refund minus fee for cancellation
        job.update_progress(0.0).unwrap();
        let cancel_refund = job.calculate_refund(JobFailureType::Canceled);
        assert_eq!(cancel_refund, 90); // 100% x 0.9

        // 100% progress - no refund for any failure type except system/timeout
        job.update_progress(100.0).unwrap();

        assert_eq!(job.calculate_refund(JobFailureType::System), 100); // Still full
        assert_eq!(job.calculate_refund(JobFailureType::Timeout), 100); // Still full
        assert_eq!(job.calculate_refund(JobFailureType::Validation), 0); // No remaining work
        assert_eq!(job.calculate_refund(JobFailureType::Canceled), 0); // No remaining work

        // Test with no credits charged
        let user_owner2 = Urn::user(user_id);
        let mut free_job = Job::new(user_owner2, user_id, None, json!({}), 0, None).unwrap();
        free_job.update_progress(50.0).unwrap();
        assert_eq!(free_job.calculate_refund(JobFailureType::System), 0);
        assert_eq!(free_job.calculate_refund(JobFailureType::Canceled), 0);
    }

    #[test]
    fn test_refund_precision_edge_cases() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test cases that verify correct FLOOR behavior according to spec
        let precision_test_cases = vec![
            // (credits, progress, expected_validation_refund, expected_cancel_refund, description)
            (101, 33.33, 67, 60, "Odd credits with fractional progress"),
            (99, 50.5, 49, 44, "Even credits with fractional progress"),
            (1, 75.0, 0, 0, "Single credit edge case"),
            (1000, 0.1, 999, 899, "Large amount with tiny progress"),
            (
                5,
                33.33,
                3,
                3,
                "Small amount with fractional progress - CORRECTED",
            ),
            (33, 33.33, 22, 19, "Matching credit amount and progress"),
            (1, 1.0, 0, 0, "Minimal progress on single credit"),
            (999, 99.9, 0, 0, "Near-complete progress"),
            (
                1001,
                66.67,
                333,
                300,
                "Large odd amount with common fraction",
            ),
        ];

        for (credits, progress, expected_validation, expected_cancel, description) in
            precision_test_cases
        {
            let mut job =
                Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
            job.update_progress(progress).unwrap();

            // Test validation refund
            let validation_refund = job.calculate_refund(JobFailureType::Validation);
            assert_eq!(
                validation_refund, expected_validation,
                "Validation refund mismatch for {}: {} credits at {}% progress",
                description, credits, progress
            );

            // Test cancellation refund
            let cancel_refund = job.calculate_refund(JobFailureType::Canceled);
            assert_eq!(
                cancel_refund, expected_cancel,
                "Cancellation refund mismatch for {}: {} credits at {}% progress",
                description, credits, progress
            );
        }
    }

    #[test]
    fn test_refund_boundary_conditions() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test zero credits
        let mut zero_job = Job::new(user_owner.clone(), user_id, None, json!({}), 0, None).unwrap();
        zero_job.update_progress(50.0).unwrap();
        assert_eq!(zero_job.calculate_refund(JobFailureType::System), 0);
        assert_eq!(zero_job.calculate_refund(JobFailureType::Timeout), 0);
        assert_eq!(zero_job.calculate_refund(JobFailureType::Validation), 0);
        assert_eq!(zero_job.calculate_refund(JobFailureType::Canceled), 0);

        // Test single credit with various progress values
        let single_credit_cases = vec![
            (0.0, 1, 0),  // 0% progress: full validation refund, 90% cancel refund
            (10.0, 0, 0), // 10% progress: 90% validation refund (0.9 -> 0), 81% cancel refund (0.729 -> 0)
            (50.0, 0, 0), // 50% progress: 50% validation refund (0.5 -> 0), 45% cancel refund (0.45 -> 0)
            (90.0, 0, 0), // 90% progress: 10% validation refund (0.1 -> 0), 9% cancel refund (0.09 -> 0)
            (99.0, 0, 0), // 99% progress: 1% validation refund (0.01 -> 0), 0.9% cancel refund (0.009 -> 0)
        ];

        for (progress, expected_validation, expected_cancel) in single_credit_cases {
            let mut single_job =
                Job::new(user_owner.clone(), user_id, None, json!({}), 1, None).unwrap();
            single_job.update_progress(progress).unwrap();

            assert_eq!(
                single_job.calculate_refund(JobFailureType::Validation),
                expected_validation,
                "Single credit validation refund at {}% progress",
                progress
            );

            assert_eq!(
                single_job.calculate_refund(JobFailureType::Canceled),
                expected_cancel,
                "Single credit cancellation refund at {}% progress",
                progress
            );
        }

        // Test maximum safe integer values
        let max_safe_credits = 1_000_000_000; // Large but safe for i32 math
        let mut large_job =
            Job::new(user_owner, user_id, None, json!({}), max_safe_credits, None).unwrap();

        // Test with small progress to ensure no overflow
        large_job.update_progress(1.0).unwrap();
        let large_validation_refund = large_job.calculate_refund(JobFailureType::Validation);
        let large_cancel_refund = large_job.calculate_refund(JobFailureType::Canceled);

        // With 1% progress: 99% should be refunded for validation, 89.1% for cancellation
        assert_eq!(large_validation_refund, 990_000_000); // 99% of 1B
        assert_eq!(large_cancel_refund, 891_000_000); // 99% * 90% of 1B
    }

    #[test]
    fn test_cancellation_minimum_charge_enforcement() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test cases where 10% minimum charge should be enforced
        let enforcement_cases = vec![
            // (credits, progress, expected_refund, expected_charge, description)
            (
                100,
                0.0,
                90,
                10,
                "Zero progress should enforce minimum 10% charge",
            ),
            (100, 5.0, 85, 15, "Low progress within normal range"),
            (100, 1.0, 89, 11, "Tiny progress should still work normally"),
            (50, 0.0, 45, 5, "Half credits with zero progress"),
            (10, 0.0, 9, 1, "Small amount with zero progress"),
            (1000, 0.1, 899, 101, "Large amount with minimal progress"),
        ];

        for (credits, progress, expected_refund, expected_charge, description) in enforcement_cases
        {
            let mut job =
                Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
            job.update_progress(progress).unwrap();

            let actual_refund = job.calculate_refund(JobFailureType::Canceled);
            let actual_charge = credits - actual_refund;

            assert_eq!(
                actual_refund, expected_refund,
                "Refund mismatch: {}",
                description
            );
            assert_eq!(
                actual_charge, expected_charge,
                "Charge mismatch: {}",
                description
            );

            // Verify minimum charge constraint
            let min_charge = (credits as f64 * 0.1).ceil() as i32;
            assert!(
                actual_charge >= min_charge || actual_charge == credits,
                "Minimum 10% charge not enforced for {}: actual charge {} < minimum {}",
                description,
                actual_charge,
                min_charge
            );
        }
    }

    #[test]
    fn test_refund_calculation_consistency() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test that refund calculations are consistent across multiple calls
        let mut job = Job::new(user_owner, user_id, None, json!({}), 150, None).unwrap();
        job.update_progress(33.33).unwrap();

        // Call refund calculation multiple times - should be deterministic
        let refunds: Vec<_> = (0..10)
            .map(|_| job.calculate_refund(JobFailureType::Validation))
            .collect();

        // All refunds should be identical
        assert!(
            refunds.iter().all(|&r| r == refunds[0]),
            "Refund calculations should be deterministic"
        );

        // Same for cancellation refunds
        let cancel_refunds: Vec<_> = (0..10)
            .map(|_| job.calculate_refund(JobFailureType::Canceled))
            .collect();

        assert!(
            cancel_refunds.iter().all(|&r| r == cancel_refunds[0]),
            "Cancellation refunds should be deterministic"
        );
    }

    #[test]
    fn test_refund_mathematical_properties() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test mathematical properties that should hold for refund calculations
        for credits in [1, 10, 100, 1000] {
            for progress in [0.0, 25.0, 50.0, 75.0, 100.0] {
                let mut job =
                    Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
                job.update_progress(progress).unwrap();

                // Property 1: System/timeout refunds should always equal charged amount
                assert_eq!(job.calculate_refund(JobFailureType::System), credits);
                assert_eq!(job.calculate_refund(JobFailureType::Timeout), credits);

                // Property 2: Refunds should never exceed charged amount
                let validation_refund = job.calculate_refund(JobFailureType::Validation);
                let cancel_refund = job.calculate_refund(JobFailureType::Canceled);

                assert!(
                    validation_refund <= credits,
                    "Validation refund {} exceeds credits {} for {}% progress",
                    validation_refund,
                    credits,
                    progress
                );
                assert!(
                    cancel_refund <= credits,
                    "Cancellation refund {} exceeds credits {} for {}% progress",
                    cancel_refund,
                    credits,
                    progress
                );

                // Property 3: Cancellation refund should never exceed validation refund
                assert!(cancel_refund <= validation_refund,
                        "Cancellation refund {} exceeds validation refund {} for {} credits at {}% progress",
                        cancel_refund, validation_refund, credits, progress);

                // Property 4: At 100% progress, validation and cancellation refunds should be 0
                if progress == 100.0 {
                    assert_eq!(validation_refund, 0);
                    assert_eq!(cancel_refund, 0);
                }

                // Property 5: At 0% progress, validation refund should equal credits
                if progress == 0.0 {
                    assert_eq!(validation_refund, credits);
                    // Cancellation should be 90% of credits (or less due to integer math)
                    assert!(cancel_refund <= (credits as f64 * 0.9).floor() as i32);
                }
            }
        }
    }

    #[test]
    fn test_progress_percentage_edge_cases() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Test various progress percentage edge cases
        let progress_cases = vec![
            (0.0, "Zero progress"),
            (0.01, "Minimal progress"),
            (33.33, "Common fraction (1/3)"),
            (66.67, "Common fraction (2/3)"),
            (99.99, "Near completion"),
            (100.0, "Full completion"),
        ];

        for (progress, description) in progress_cases {
            job.update_progress(progress).unwrap();

            // Verify progress is set correctly
            assert!(
                (job.get_progress_percent() - progress).abs() < 0.01,
                "Progress not set correctly for {}: expected {}, got {}",
                description,
                progress,
                job.get_progress_percent()
            );

            // Verify refunds are calculated correctly
            let validation_refund = job.calculate_refund(JobFailureType::Validation);
            let cancel_refund = job.calculate_refund(JobFailureType::Canceled);

            // Basic sanity checks
            assert!(
                validation_refund >= 0,
                "Validation refund negative for {}",
                description
            );
            assert!(
                cancel_refund >= 0,
                "Cancellation refund negative for {}",
                description
            );
            assert!(
                validation_refund <= 100,
                "Validation refund too high for {}",
                description
            );
            assert!(
                cancel_refund <= 100,
                "Cancellation refund too high for {}",
                description
            );
        }
    }

    #[test]
    fn test_usage_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let period = "2025-01".to_string();

        let usage = Usage::new(owner.clone(), period.clone()).unwrap();

        assert_eq!(usage.owner_urn().unwrap(), owner);
        assert_eq!(usage.period, period);
        assert_eq!(usage.renders_count, 0);
        assert_eq!(usage.credits_used, 0);
        assert_eq!(usage.net_credits(), 0);
    }

    #[test]
    fn test_usage_period_validation() {
        let owner = Urn::user(Uuid::new_v4());

        // Invalid period formats
        assert!(Usage::new(owner.clone(), "2025".to_string()).is_err());
        assert!(Usage::new(owner.clone(), "2025-1".to_string()).is_err());
        assert!(Usage::new(owner.clone(), "2025-13".to_string()).is_err());
        assert!(Usage::new(owner.clone(), "25-01".to_string()).is_err());

        // Valid period
        assert!(Usage::new(owner, "2025-01".to_string()).is_ok());
    }
}
