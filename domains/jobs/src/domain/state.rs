//! State machines for Jobs domain entities
//!
//! This module implements formal state machines as defined in the specification
//! (docs/spec/05_Relationships_States.md). Each state machine defines:
//! - Valid states
//! - Events that trigger transitions
//! - Guard conditions for transitions
//! - Terminal states

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
}
