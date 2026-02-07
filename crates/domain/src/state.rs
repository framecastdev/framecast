//! State machines for Framecast entities
//!
//! This module implements formal state machines as defined in the specification
//! (docs/spec/05_Relationships_States.md). Each state machine defines:
//! - Valid states
//! - Events that trigger transitions
//! - Guard conditions for transitions
//! - Terminal states

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during state transitions
#[derive(Debug, Error, Clone, PartialEq)]
pub enum StateError {
    #[error("Invalid transition: cannot transition from {from} to {to} via {event}")]
    InvalidTransition {
        from: String,
        to: String,
        event: String,
    },

    #[error("Guard condition failed: {0}")]
    GuardFailed(String),

    #[error("Terminal state: {0} is a terminal state and cannot transition")]
    TerminalState(String),
}

// ============================================================================
// Job State Machine
// ============================================================================

/// Job status states as defined in spec section 6.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobState {
    Queued,
    Processing,
    Completed,
    Failed,
    Canceled,
}

impl JobState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [JobState] {
        match self {
            Self::Queued => &[Self::Processing, Self::Canceled],
            Self::Processing => &[Self::Completed, Self::Failed, Self::Canceled],
            Self::Completed => &[],
            Self::Failed => &[],
            Self::Canceled => &[],
        }
    }
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Processing => write!(f, "processing"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

/// Events that trigger job state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum JobEvent {
    /// Worker picks up the job for processing
    WorkerPicksUp,
    /// Job completes successfully
    Success,
    /// Job fails with an error
    Failure,
    /// Job is canceled by user or system
    Cancel,
}

impl std::fmt::Display for JobEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkerPicksUp => write!(f, "worker_picks_up"),
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::Cancel => write!(f, "cancel"),
        }
    }
}

/// Job state machine
pub struct JobStateMachine;

impl JobStateMachine {
    /// Attempt a state transition
    ///
    /// Returns the new state if the transition is valid, or an error otherwise.
    pub fn transition(current: JobState, event: JobEvent) -> Result<JobState, StateError> {
        // Check for terminal state
        if current.is_terminal() {
            return Err(StateError::TerminalState(current.to_string()));
        }

        let next = match (&current, &event) {
            // From Queued
            (JobState::Queued, JobEvent::WorkerPicksUp) => JobState::Processing,
            (JobState::Queued, JobEvent::Cancel) => JobState::Canceled,

            // From Processing
            (JobState::Processing, JobEvent::Success) => JobState::Completed,
            (JobState::Processing, JobEvent::Failure) => JobState::Failed,
            (JobState::Processing, JobEvent::Cancel) => JobState::Canceled,

            // Invalid transitions
            _ => {
                return Err(StateError::InvalidTransition {
                    from: current.to_string(),
                    to: "unknown".to_string(),
                    event: event.to_string(),
                });
            }
        };

        Ok(next)
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition(current: JobState, event: &JobEvent) -> bool {
        Self::transition(current, event.clone()).is_ok()
    }
}

// ============================================================================
// Project State Machine
// ============================================================================

/// Project status states as defined in spec section 6.3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectState {
    Draft,
    Rendering,
    Completed,
    Archived,
}

impl ProjectState {
    /// Check if this is a terminal state (Project has no terminal states)
    pub fn is_terminal(&self) -> bool {
        false // Project has no terminal states per spec
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [ProjectState] {
        match self {
            Self::Draft => &[Self::Rendering, Self::Archived],
            Self::Rendering => &[Self::Completed, Self::Draft],
            Self::Completed => &[Self::Archived, Self::Rendering],
            Self::Archived => &[Self::Draft],
        }
    }
}

impl std::fmt::Display for ProjectState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Rendering => write!(f, "rendering"),
            Self::Completed => write!(f, "completed"),
            Self::Archived => write!(f, "archived"),
        }
    }
}

/// Events that trigger project state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectEvent {
    /// Start rendering the project
    Render,
    /// Associated job completed successfully
    JobCompleted,
    /// Associated job failed
    JobFailed,
    /// Associated job was canceled
    JobCanceled,
    /// Archive the project
    Archive,
    /// Unarchive the project
    Unarchive,
}

impl std::fmt::Display for ProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Render => write!(f, "render"),
            Self::JobCompleted => write!(f, "job_completed"),
            Self::JobFailed => write!(f, "job_failed"),
            Self::JobCanceled => write!(f, "job_canceled"),
            Self::Archive => write!(f, "archive"),
            Self::Unarchive => write!(f, "unarchive"),
        }
    }
}

/// Project state machine
pub struct ProjectStateMachine;

impl ProjectStateMachine {
    /// Attempt a state transition
    pub fn transition(
        current: ProjectState,
        event: ProjectEvent,
    ) -> Result<ProjectState, StateError> {
        let next = match (&current, &event) {
            // From Draft
            (ProjectState::Draft, ProjectEvent::Render) => ProjectState::Rendering,
            (ProjectState::Draft, ProjectEvent::Archive) => ProjectState::Archived,

            // From Rendering
            (ProjectState::Rendering, ProjectEvent::JobCompleted) => ProjectState::Completed,
            (ProjectState::Rendering, ProjectEvent::JobFailed) => ProjectState::Draft,
            (ProjectState::Rendering, ProjectEvent::JobCanceled) => ProjectState::Draft,

            // From Completed
            (ProjectState::Completed, ProjectEvent::Archive) => ProjectState::Archived,
            (ProjectState::Completed, ProjectEvent::Render) => ProjectState::Rendering, // Re-render

            // From Archived
            (ProjectState::Archived, ProjectEvent::Unarchive) => ProjectState::Draft,

            // Invalid transitions
            _ => {
                return Err(StateError::InvalidTransition {
                    from: current.to_string(),
                    to: "unknown".to_string(),
                    event: event.to_string(),
                });
            }
        };

        Ok(next)
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition(current: ProjectState, event: &ProjectEvent) -> bool {
        Self::transition(current, event.clone()).is_ok()
    }
}

// ============================================================================
// Invitation State Machine
// ============================================================================

/// Invitation states as defined in spec section 6.4
/// Note: This is a derived/computed state, not stored directly
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvitationState {
    Pending,
    Accepted,
    Declined,
    Expired,
    Revoked,
}

impl InvitationState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Declined | Self::Expired | Self::Revoked
        )
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [InvitationState] {
        match self {
            Self::Pending => &[Self::Accepted, Self::Declined, Self::Expired, Self::Revoked],
            Self::Accepted => &[],
            Self::Declined => &[],
            Self::Expired => &[],
            Self::Revoked => &[],
        }
    }
}

impl std::fmt::Display for InvitationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Accepted => write!(f, "accepted"),
            Self::Declined => write!(f, "declined"),
            Self::Expired => write!(f, "expired"),
            Self::Revoked => write!(f, "revoked"),
        }
    }
}

/// Events that trigger invitation state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum InvitationEvent {
    /// User accepts the invitation
    Accept,
    /// User declines the invitation
    Decline,
    /// Invitation expires (automatic when expires_at is reached)
    Expire,
    /// Admin revokes the invitation
    Revoke,
}

impl std::fmt::Display for InvitationEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accept => write!(f, "accept"),
            Self::Decline => write!(f, "decline"),
            Self::Expire => write!(f, "expire"),
            Self::Revoke => write!(f, "revoke"),
        }
    }
}

/// Guard context for invitation transitions
#[derive(Debug, Clone)]
pub struct InvitationGuardContext {
    /// Whether the invitation has expired (expires_at < now)
    pub is_expired: bool,
}

/// Invitation state machine
pub struct InvitationStateMachine;

impl InvitationStateMachine {
    /// Attempt a state transition with guard conditions
    pub fn transition(
        current: InvitationState,
        event: InvitationEvent,
        context: Option<&InvitationGuardContext>,
    ) -> Result<InvitationState, StateError> {
        // Check for terminal state
        if current.is_terminal() {
            return Err(StateError::TerminalState(current.to_string()));
        }

        let next = match (&current, &event) {
            // From Pending
            (InvitationState::Pending, InvitationEvent::Accept) => {
                // Guard: invitation must not be expired
                if let Some(ctx) = context {
                    if ctx.is_expired {
                        return Err(StateError::GuardFailed(
                            "Cannot accept expired invitation".to_string(),
                        ));
                    }
                }
                InvitationState::Accepted
            }
            (InvitationState::Pending, InvitationEvent::Decline) => InvitationState::Declined,
            (InvitationState::Pending, InvitationEvent::Expire) => InvitationState::Expired,
            (InvitationState::Pending, InvitationEvent::Revoke) => InvitationState::Revoked,

            // Invalid transitions
            _ => {
                return Err(StateError::InvalidTransition {
                    from: current.to_string(),
                    to: "unknown".to_string(),
                    event: event.to_string(),
                });
            }
        };

        Ok(next)
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition(
        current: InvitationState,
        event: &InvitationEvent,
        context: Option<&InvitationGuardContext>,
    ) -> bool {
        Self::transition(current, event.clone(), context).is_ok()
    }
}

// ============================================================================
// WebhookDelivery State Machine
// ============================================================================

/// WebhookDelivery states as defined in spec section 6.5
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookDeliveryState {
    Pending,
    Attempting,
    Delivered,
    Retrying,
    Failed,
}

impl WebhookDeliveryState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Delivered | Self::Failed)
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [WebhookDeliveryState] {
        match self {
            Self::Pending => &[Self::Attempting],
            Self::Attempting => &[Self::Delivered, Self::Retrying, Self::Failed],
            Self::Retrying => &[Self::Attempting, Self::Failed],
            Self::Delivered => &[],
            Self::Failed => &[],
        }
    }
}

impl std::fmt::Display for WebhookDeliveryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Attempting => write!(f, "attempting"),
            Self::Delivered => write!(f, "delivered"),
            Self::Retrying => write!(f, "retrying"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Events that trigger webhook delivery state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum WebhookDeliveryEvent {
    /// Start an attempt to deliver
    Attempt,
    /// Received 2xx success response
    Success,
    /// Received 5xx or timeout, should retry
    Retry,
    /// Received 4xx permanent failure
    PermanentFailure,
    /// Max retry attempts exceeded
    MaxAttemptsExceeded,
}

impl std::fmt::Display for WebhookDeliveryEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Attempt => write!(f, "attempt"),
            Self::Success => write!(f, "success"),
            Self::Retry => write!(f, "retry"),
            Self::PermanentFailure => write!(f, "permanent_failure"),
            Self::MaxAttemptsExceeded => write!(f, "max_attempts_exceeded"),
        }
    }
}

/// Guard context for webhook delivery transitions
#[derive(Debug, Clone)]
pub struct WebhookDeliveryGuardContext {
    /// Current attempt count
    pub attempt_count: u32,
    /// Maximum allowed attempts
    pub max_attempts: u32,
}

impl Default for WebhookDeliveryGuardContext {
    fn default() -> Self {
        Self {
            attempt_count: 0,
            max_attempts: 5,
        }
    }
}

/// WebhookDelivery state machine
pub struct WebhookDeliveryStateMachine;

impl WebhookDeliveryStateMachine {
    /// Attempt a state transition with guard conditions
    pub fn transition(
        current: WebhookDeliveryState,
        event: WebhookDeliveryEvent,
        context: Option<&WebhookDeliveryGuardContext>,
    ) -> Result<WebhookDeliveryState, StateError> {
        // Check for terminal state
        if current.is_terminal() {
            return Err(StateError::TerminalState(current.to_string()));
        }

        let next = match (&current, &event) {
            // From Pending
            (WebhookDeliveryState::Pending, WebhookDeliveryEvent::Attempt) => {
                WebhookDeliveryState::Attempting
            }

            // From Attempting
            (WebhookDeliveryState::Attempting, WebhookDeliveryEvent::Success) => {
                WebhookDeliveryState::Delivered
            }
            (WebhookDeliveryState::Attempting, WebhookDeliveryEvent::Retry) => {
                WebhookDeliveryState::Retrying
            }
            (WebhookDeliveryState::Attempting, WebhookDeliveryEvent::PermanentFailure) => {
                WebhookDeliveryState::Failed
            }

            // From Retrying
            (WebhookDeliveryState::Retrying, WebhookDeliveryEvent::Attempt) => {
                // Guard: check if max attempts exceeded
                if let Some(ctx) = context {
                    if ctx.attempt_count >= ctx.max_attempts {
                        return Err(StateError::GuardFailed(
                            "Max retry attempts exceeded".to_string(),
                        ));
                    }
                }
                WebhookDeliveryState::Attempting
            }
            (WebhookDeliveryState::Retrying, WebhookDeliveryEvent::MaxAttemptsExceeded) => {
                WebhookDeliveryState::Failed
            }

            // Invalid transitions
            _ => {
                return Err(StateError::InvalidTransition {
                    from: current.to_string(),
                    to: "unknown".to_string(),
                    event: event.to_string(),
                });
            }
        };

        Ok(next)
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition(
        current: WebhookDeliveryState,
        event: &WebhookDeliveryEvent,
        context: Option<&WebhookDeliveryGuardContext>,
    ) -> bool {
        Self::transition(current, event.clone(), context).is_ok()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Job State Machine Tests
    // ------------------------------------------------------------------------

    mod job_state_machine {
        use super::*;

        #[test]
        fn test_valid_queued_to_processing() {
            let result = JobStateMachine::transition(JobState::Queued, JobEvent::WorkerPicksUp);
            assert_eq!(result, Ok(JobState::Processing));
        }

        #[test]
        fn test_valid_queued_to_canceled() {
            let result = JobStateMachine::transition(JobState::Queued, JobEvent::Cancel);
            assert_eq!(result, Ok(JobState::Canceled));
        }

        #[test]
        fn test_valid_processing_to_completed() {
            let result = JobStateMachine::transition(JobState::Processing, JobEvent::Success);
            assert_eq!(result, Ok(JobState::Completed));
        }

        #[test]
        fn test_valid_processing_to_failed() {
            let result = JobStateMachine::transition(JobState::Processing, JobEvent::Failure);
            assert_eq!(result, Ok(JobState::Failed));
        }

        #[test]
        fn test_valid_processing_to_canceled() {
            let result = JobStateMachine::transition(JobState::Processing, JobEvent::Cancel);
            assert_eq!(result, Ok(JobState::Canceled));
        }

        #[test]
        fn test_invalid_queued_to_completed() {
            let result = JobStateMachine::transition(JobState::Queued, JobEvent::Success);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_invalid_queued_to_failed() {
            let result = JobStateMachine::transition(JobState::Queued, JobEvent::Failure);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_terminal_completed_cannot_transition() {
            let result = JobStateMachine::transition(JobState::Completed, JobEvent::Cancel);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_failed_cannot_transition() {
            let result = JobStateMachine::transition(JobState::Failed, JobEvent::Success);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_canceled_cannot_transition() {
            let result = JobStateMachine::transition(JobState::Canceled, JobEvent::WorkerPicksUp);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_is_terminal() {
            assert!(!JobState::Queued.is_terminal());
            assert!(!JobState::Processing.is_terminal());
            assert!(JobState::Completed.is_terminal());
            assert!(JobState::Failed.is_terminal());
            assert!(JobState::Canceled.is_terminal());
        }

        #[test]
        fn test_can_transition() {
            assert!(JobStateMachine::can_transition(
                JobState::Queued,
                &JobEvent::WorkerPicksUp
            ));
            assert!(!JobStateMachine::can_transition(
                JobState::Queued,
                &JobEvent::Success
            ));
            assert!(!JobStateMachine::can_transition(
                JobState::Completed,
                &JobEvent::Cancel
            ));
        }

        #[test]
        fn test_valid_transitions_from_queued() {
            let transitions = JobState::Queued.valid_transitions();
            assert!(transitions.contains(&JobState::Processing));
            assert!(transitions.contains(&JobState::Canceled));
            assert_eq!(transitions.len(), 2);
        }

        #[test]
        fn test_valid_transitions_from_processing() {
            let transitions = JobState::Processing.valid_transitions();
            assert!(transitions.contains(&JobState::Completed));
            assert!(transitions.contains(&JobState::Failed));
            assert!(transitions.contains(&JobState::Canceled));
            assert_eq!(transitions.len(), 3);
        }

        #[test]
        fn test_terminal_states_have_no_transitions() {
            assert!(JobState::Completed.valid_transitions().is_empty());
            assert!(JobState::Failed.valid_transitions().is_empty());
            assert!(JobState::Canceled.valid_transitions().is_empty());
        }
    }

    // ------------------------------------------------------------------------
    // Project State Machine Tests
    // ------------------------------------------------------------------------

    mod project_state_machine {
        use super::*;

        #[test]
        fn test_valid_draft_to_rendering() {
            let result = ProjectStateMachine::transition(ProjectState::Draft, ProjectEvent::Render);
            assert_eq!(result, Ok(ProjectState::Rendering));
        }

        #[test]
        fn test_valid_draft_to_archived() {
            let result =
                ProjectStateMachine::transition(ProjectState::Draft, ProjectEvent::Archive);
            assert_eq!(result, Ok(ProjectState::Archived));
        }

        #[test]
        fn test_valid_rendering_to_completed() {
            let result = ProjectStateMachine::transition(
                ProjectState::Rendering,
                ProjectEvent::JobCompleted,
            );
            assert_eq!(result, Ok(ProjectState::Completed));
        }

        #[test]
        fn test_valid_rendering_to_draft_on_failure() {
            let result =
                ProjectStateMachine::transition(ProjectState::Rendering, ProjectEvent::JobFailed);
            assert_eq!(result, Ok(ProjectState::Draft));
        }

        #[test]
        fn test_valid_rendering_to_draft_on_cancel() {
            let result =
                ProjectStateMachine::transition(ProjectState::Rendering, ProjectEvent::JobCanceled);
            assert_eq!(result, Ok(ProjectState::Draft));
        }

        #[test]
        fn test_valid_completed_to_archived() {
            let result =
                ProjectStateMachine::transition(ProjectState::Completed, ProjectEvent::Archive);
            assert_eq!(result, Ok(ProjectState::Archived));
        }

        #[test]
        fn test_valid_completed_to_rendering_rerender() {
            let result =
                ProjectStateMachine::transition(ProjectState::Completed, ProjectEvent::Render);
            assert_eq!(result, Ok(ProjectState::Rendering));
        }

        #[test]
        fn test_valid_archived_to_draft() {
            let result =
                ProjectStateMachine::transition(ProjectState::Archived, ProjectEvent::Unarchive);
            assert_eq!(result, Ok(ProjectState::Draft));
        }

        #[test]
        fn test_invalid_draft_to_completed() {
            let result =
                ProjectStateMachine::transition(ProjectState::Draft, ProjectEvent::JobCompleted);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_invalid_archived_to_completed() {
            let result =
                ProjectStateMachine::transition(ProjectState::Archived, ProjectEvent::JobCompleted);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_project_valid_transitions() {
            // Kill mutant: ProjectState::valid_transitions -> Vec::leak(Vec::new())
            let draft = ProjectState::Draft.valid_transitions();
            assert!(!draft.is_empty());
            assert!(draft.contains(&ProjectState::Rendering));
            assert!(draft.contains(&ProjectState::Archived));
            assert_eq!(draft.len(), 2);

            let rendering = ProjectState::Rendering.valid_transitions();
            assert!(!rendering.is_empty());
            assert!(rendering.contains(&ProjectState::Completed));
            assert!(rendering.contains(&ProjectState::Draft));
            assert_eq!(rendering.len(), 2);

            let completed = ProjectState::Completed.valid_transitions();
            assert!(!completed.is_empty());
            assert!(completed.contains(&ProjectState::Archived));
            assert!(completed.contains(&ProjectState::Rendering));
            assert_eq!(completed.len(), 2);

            let archived = ProjectState::Archived.valid_transitions();
            assert!(!archived.is_empty());
            assert!(archived.contains(&ProjectState::Draft));
            assert_eq!(archived.len(), 1);
        }

        #[test]
        fn test_project_can_transition() {
            // Kill mutant: ProjectStateMachine::can_transition -> true / false
            // Valid transitions
            assert!(ProjectStateMachine::can_transition(
                ProjectState::Draft,
                &ProjectEvent::Render
            ));
            assert!(ProjectStateMachine::can_transition(
                ProjectState::Draft,
                &ProjectEvent::Archive
            ));
            assert!(ProjectStateMachine::can_transition(
                ProjectState::Completed,
                &ProjectEvent::Archive
            ));

            // Invalid transitions
            assert!(!ProjectStateMachine::can_transition(
                ProjectState::Draft,
                &ProjectEvent::JobCompleted
            ));
            assert!(!ProjectStateMachine::can_transition(
                ProjectState::Archived,
                &ProjectEvent::Render
            ));
            assert!(!ProjectStateMachine::can_transition(
                ProjectState::Completed,
                &ProjectEvent::Unarchive
            ));
        }

        #[test]
        fn test_project_has_no_terminal_states() {
            assert!(!ProjectState::Draft.is_terminal());
            assert!(!ProjectState::Rendering.is_terminal());
            assert!(!ProjectState::Completed.is_terminal());
            assert!(!ProjectState::Archived.is_terminal());
        }
    }

    // ------------------------------------------------------------------------
    // Invitation State Machine Tests
    // ------------------------------------------------------------------------

    mod invitation_state_machine {
        use super::*;

        #[test]
        fn test_valid_pending_to_accepted() {
            let ctx = InvitationGuardContext { is_expired: false };
            let result = InvitationStateMachine::transition(
                InvitationState::Pending,
                InvitationEvent::Accept,
                Some(&ctx),
            );
            assert_eq!(result, Ok(InvitationState::Accepted));
        }

        #[test]
        fn test_valid_pending_to_expired() {
            let result = InvitationStateMachine::transition(
                InvitationState::Pending,
                InvitationEvent::Expire,
                None,
            );
            assert_eq!(result, Ok(InvitationState::Expired));
        }

        #[test]
        fn test_valid_pending_to_revoked() {
            let result = InvitationStateMachine::transition(
                InvitationState::Pending,
                InvitationEvent::Revoke,
                None,
            );
            assert_eq!(result, Ok(InvitationState::Revoked));
        }

        #[test]
        fn test_guard_fails_accept_expired_invitation() {
            let ctx = InvitationGuardContext { is_expired: true };
            let result = InvitationStateMachine::transition(
                InvitationState::Pending,
                InvitationEvent::Accept,
                Some(&ctx),
            );
            assert!(matches!(result, Err(StateError::GuardFailed(_))));
        }

        #[test]
        fn test_terminal_accepted_cannot_transition() {
            let result = InvitationStateMachine::transition(
                InvitationState::Accepted,
                InvitationEvent::Revoke,
                None,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_expired_cannot_transition() {
            let result = InvitationStateMachine::transition(
                InvitationState::Expired,
                InvitationEvent::Accept,
                None,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_revoked_cannot_transition() {
            let result = InvitationStateMachine::transition(
                InvitationState::Revoked,
                InvitationEvent::Accept,
                None,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_valid_pending_to_declined() {
            let result = InvitationStateMachine::transition(
                InvitationState::Pending,
                InvitationEvent::Decline,
                None,
            );
            assert_eq!(result, Ok(InvitationState::Declined));
        }

        #[test]
        fn test_terminal_declined_cannot_transition() {
            let result = InvitationStateMachine::transition(
                InvitationState::Declined,
                InvitationEvent::Accept,
                None,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_is_terminal() {
            assert!(!InvitationState::Pending.is_terminal());
            assert!(InvitationState::Accepted.is_terminal());
            assert!(InvitationState::Declined.is_terminal());
            assert!(InvitationState::Expired.is_terminal());
            assert!(InvitationState::Revoked.is_terminal());
        }

        #[test]
        fn test_invitation_valid_transitions() {
            // Kill mutant: InvitationState::valid_transitions -> Vec::leak(Vec::new())
            let pending = InvitationState::Pending.valid_transitions();
            assert!(!pending.is_empty());
            assert_eq!(pending.len(), 4);
            assert!(pending.contains(&InvitationState::Accepted));
            assert!(pending.contains(&InvitationState::Declined));
            assert!(pending.contains(&InvitationState::Expired));
            assert!(pending.contains(&InvitationState::Revoked));

            // Terminal states should have no transitions
            assert!(InvitationState::Accepted.valid_transitions().is_empty());
            assert!(InvitationState::Declined.valid_transitions().is_empty());
            assert!(InvitationState::Expired.valid_transitions().is_empty());
            assert!(InvitationState::Revoked.valid_transitions().is_empty());
        }

        #[test]
        fn test_invitation_can_transition() {
            // Kill mutant: InvitationStateMachine::can_transition -> true / false
            let ctx = InvitationGuardContext { is_expired: false };

            // Valid transitions
            assert!(InvitationStateMachine::can_transition(
                InvitationState::Pending,
                &InvitationEvent::Accept,
                Some(&ctx)
            ));
            assert!(InvitationStateMachine::can_transition(
                InvitationState::Pending,
                &InvitationEvent::Decline,
                None
            ));
            assert!(InvitationStateMachine::can_transition(
                InvitationState::Pending,
                &InvitationEvent::Revoke,
                None
            ));

            // Invalid transitions (from terminal states)
            assert!(!InvitationStateMachine::can_transition(
                InvitationState::Accepted,
                &InvitationEvent::Revoke,
                None
            ));
            assert!(!InvitationStateMachine::can_transition(
                InvitationState::Declined,
                &InvitationEvent::Accept,
                None
            ));
            assert!(!InvitationStateMachine::can_transition(
                InvitationState::Expired,
                &InvitationEvent::Accept,
                None
            ));
        }
    }

    // ------------------------------------------------------------------------
    // WebhookDelivery State Machine Tests
    // ------------------------------------------------------------------------

    mod webhook_delivery_state_machine {
        use super::*;

        #[test]
        fn test_valid_pending_to_attempting() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Pending,
                WebhookDeliveryEvent::Attempt,
                None,
            );
            assert_eq!(result, Ok(WebhookDeliveryState::Attempting));
        }

        #[test]
        fn test_valid_attempting_to_delivered() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Attempting,
                WebhookDeliveryEvent::Success,
                None,
            );
            assert_eq!(result, Ok(WebhookDeliveryState::Delivered));
        }

        #[test]
        fn test_valid_attempting_to_retrying() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Attempting,
                WebhookDeliveryEvent::Retry,
                None,
            );
            assert_eq!(result, Ok(WebhookDeliveryState::Retrying));
        }

        #[test]
        fn test_valid_attempting_to_failed_permanent() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Attempting,
                WebhookDeliveryEvent::PermanentFailure,
                None,
            );
            assert_eq!(result, Ok(WebhookDeliveryState::Failed));
        }

        #[test]
        fn test_valid_retrying_to_attempting() {
            let ctx = WebhookDeliveryGuardContext {
                attempt_count: 2,
                max_attempts: 5,
            };
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Retrying,
                WebhookDeliveryEvent::Attempt,
                Some(&ctx),
            );
            assert_eq!(result, Ok(WebhookDeliveryState::Attempting));
        }

        #[test]
        fn test_guard_fails_max_attempts_exceeded() {
            let ctx = WebhookDeliveryGuardContext {
                attempt_count: 5,
                max_attempts: 5,
            };
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Retrying,
                WebhookDeliveryEvent::Attempt,
                Some(&ctx),
            );
            assert!(matches!(result, Err(StateError::GuardFailed(_))));
        }

        #[test]
        fn test_valid_retrying_to_failed_max_exceeded() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Retrying,
                WebhookDeliveryEvent::MaxAttemptsExceeded,
                None,
            );
            assert_eq!(result, Ok(WebhookDeliveryState::Failed));
        }

        #[test]
        fn test_terminal_delivered_cannot_transition() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Delivered,
                WebhookDeliveryEvent::Retry,
                None,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_failed_cannot_transition() {
            let result = WebhookDeliveryStateMachine::transition(
                WebhookDeliveryState::Failed,
                WebhookDeliveryEvent::Attempt,
                None,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_is_terminal() {
            assert!(!WebhookDeliveryState::Pending.is_terminal());
            assert!(!WebhookDeliveryState::Attempting.is_terminal());
            assert!(!WebhookDeliveryState::Retrying.is_terminal());
            assert!(WebhookDeliveryState::Delivered.is_terminal());
            assert!(WebhookDeliveryState::Failed.is_terminal());
        }

        #[test]
        fn test_webhook_delivery_valid_transitions() {
            // Kill mutant: WebhookDeliveryState::valid_transitions -> Vec::leak(Vec::new())
            let pending = WebhookDeliveryState::Pending.valid_transitions();
            assert!(!pending.is_empty());
            assert_eq!(pending.len(), 1);
            assert!(pending.contains(&WebhookDeliveryState::Attempting));

            let attempting = WebhookDeliveryState::Attempting.valid_transitions();
            assert!(!attempting.is_empty());
            assert_eq!(attempting.len(), 3);

            let retrying = WebhookDeliveryState::Retrying.valid_transitions();
            assert!(!retrying.is_empty());
            assert_eq!(retrying.len(), 2);

            // Terminal states
            assert!(WebhookDeliveryState::Delivered
                .valid_transitions()
                .is_empty());
            assert!(WebhookDeliveryState::Failed.valid_transitions().is_empty());
        }

        #[test]
        fn test_webhook_delivery_can_transition() {
            // Kill mutant: WebhookDeliveryStateMachine::can_transition -> true / false
            // Valid transitions
            assert!(WebhookDeliveryStateMachine::can_transition(
                WebhookDeliveryState::Pending,
                &WebhookDeliveryEvent::Attempt,
                None
            ));
            assert!(WebhookDeliveryStateMachine::can_transition(
                WebhookDeliveryState::Attempting,
                &WebhookDeliveryEvent::Success,
                None
            ));

            // Invalid transitions
            assert!(!WebhookDeliveryStateMachine::can_transition(
                WebhookDeliveryState::Delivered,
                &WebhookDeliveryEvent::Retry,
                None
            ));
            assert!(!WebhookDeliveryStateMachine::can_transition(
                WebhookDeliveryState::Failed,
                &WebhookDeliveryEvent::Attempt,
                None
            ));
            assert!(!WebhookDeliveryStateMachine::can_transition(
                WebhookDeliveryState::Pending,
                &WebhookDeliveryEvent::Success,
                None
            ));
        }
    }
}
