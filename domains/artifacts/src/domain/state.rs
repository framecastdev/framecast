//! State machine for artifact status transitions
//!
//! Artifact states: Pending → Ready | Failed; Failed → Pending (retry)
//! Ready is a terminal state.

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

/// Artifact states as defined in spec section 6.7
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactState {
    Pending,
    Ready,
    Failed,
}

impl ArtifactState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [ArtifactState] {
        match self {
            Self::Pending => &[Self::Ready, Self::Failed],
            Self::Ready => &[],
            Self::Failed => &[Self::Pending],
        }
    }
}

impl std::fmt::Display for ArtifactState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Events that trigger artifact state transitions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArtifactEvent {
    /// Processing/upload completed successfully
    Complete,
    /// Processing/upload failed
    Fail,
    /// Retry a failed artifact
    Retry,
}

impl std::fmt::Display for ArtifactEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complete => write!(f, "complete"),
            Self::Fail => write!(f, "fail"),
            Self::Retry => write!(f, "retry"),
        }
    }
}

/// Artifact state machine
pub struct ArtifactStateMachine;

impl ArtifactStateMachine {
    /// Attempt a state transition
    pub fn transition(
        current: ArtifactState,
        event: ArtifactEvent,
    ) -> Result<ArtifactState, StateError> {
        if current.is_terminal() {
            return Err(StateError::TerminalState(current.to_string()));
        }

        let next = match (&current, &event) {
            (ArtifactState::Pending, ArtifactEvent::Complete) => ArtifactState::Ready,
            (ArtifactState::Pending, ArtifactEvent::Fail) => ArtifactState::Failed,
            (ArtifactState::Failed, ArtifactEvent::Retry) => ArtifactState::Pending,
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
    pub fn can_transition(current: ArtifactState, event: &ArtifactEvent) -> bool {
        Self::transition(current, *event).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod artifact_state_machine {
        use super::*;

        #[test]
        fn test_pending_to_ready() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Pending, ArtifactEvent::Complete);
            assert_eq!(result, Ok(ArtifactState::Ready));
        }

        #[test]
        fn test_pending_to_failed() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Pending, ArtifactEvent::Fail);
            assert_eq!(result, Ok(ArtifactState::Failed));
        }

        #[test]
        fn test_failed_to_pending_retry() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Failed, ArtifactEvent::Retry);
            assert_eq!(result, Ok(ArtifactState::Pending));
        }

        #[test]
        fn test_ready_is_terminal() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Ready, ArtifactEvent::Fail);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_ready_cannot_retry() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Ready, ArtifactEvent::Retry);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_pending_cannot_retry() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Pending, ArtifactEvent::Retry);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_failed_cannot_complete() {
            let result =
                ArtifactStateMachine::transition(ArtifactState::Failed, ArtifactEvent::Complete);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_is_terminal() {
            assert!(!ArtifactState::Pending.is_terminal());
            assert!(ArtifactState::Ready.is_terminal());
            assert!(!ArtifactState::Failed.is_terminal());
        }

        #[test]
        fn test_valid_transitions() {
            let pending = ArtifactState::Pending.valid_transitions();
            assert_eq!(pending.len(), 2);
            assert!(pending.contains(&ArtifactState::Ready));
            assert!(pending.contains(&ArtifactState::Failed));

            assert!(ArtifactState::Ready.valid_transitions().is_empty());

            let failed = ArtifactState::Failed.valid_transitions();
            assert_eq!(failed.len(), 1);
            assert!(failed.contains(&ArtifactState::Pending));
        }

        #[test]
        fn test_can_transition() {
            assert!(ArtifactStateMachine::can_transition(
                ArtifactState::Pending,
                &ArtifactEvent::Complete
            ));
            assert!(ArtifactStateMachine::can_transition(
                ArtifactState::Pending,
                &ArtifactEvent::Fail
            ));
            assert!(!ArtifactStateMachine::can_transition(
                ArtifactState::Pending,
                &ArtifactEvent::Retry
            ));
            assert!(!ArtifactStateMachine::can_transition(
                ArtifactState::Ready,
                &ArtifactEvent::Complete
            ));
            assert!(ArtifactStateMachine::can_transition(
                ArtifactState::Failed,
                &ArtifactEvent::Retry
            ));
            assert!(!ArtifactStateMachine::can_transition(
                ArtifactState::Failed,
                &ArtifactEvent::Complete
            ));
        }

        #[test]
        fn test_state_display() {
            assert_eq!(ArtifactState::Pending.to_string(), "pending");
            assert_eq!(ArtifactState::Ready.to_string(), "ready");
            assert_eq!(ArtifactState::Failed.to_string(), "failed");
        }

        #[test]
        fn test_event_display() {
            assert_eq!(ArtifactEvent::Complete.to_string(), "complete");
            assert_eq!(ArtifactEvent::Fail.to_string(), "fail");
            assert_eq!(ArtifactEvent::Retry.to_string(), "retry");
        }
    }
}
