//! State machine for artifact status transitions
//!
//! Artifact states: Pending → Ready | Failed; Failed → Pending (retry)
//! Ready is a terminal state.

pub use framecast_common::StateError;

use crate::domain::entities::ArtifactStatus;

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
        current: ArtifactStatus,
        event: ArtifactEvent,
    ) -> Result<ArtifactStatus, StateError> {
        if current.is_terminal() {
            return Err(StateError::TerminalState(current.to_string()));
        }

        let next = match (&current, &event) {
            (ArtifactStatus::Pending, ArtifactEvent::Complete) => ArtifactStatus::Ready,
            (ArtifactStatus::Pending, ArtifactEvent::Fail) => ArtifactStatus::Failed,
            (ArtifactStatus::Failed, ArtifactEvent::Retry) => ArtifactStatus::Pending,
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
    pub fn can_transition(current: ArtifactStatus, event: &ArtifactEvent) -> bool {
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
                ArtifactStateMachine::transition(ArtifactStatus::Pending, ArtifactEvent::Complete);
            assert_eq!(result, Ok(ArtifactStatus::Ready));
        }

        #[test]
        fn test_pending_to_failed() {
            let result =
                ArtifactStateMachine::transition(ArtifactStatus::Pending, ArtifactEvent::Fail);
            assert_eq!(result, Ok(ArtifactStatus::Failed));
        }

        #[test]
        fn test_failed_to_pending_retry() {
            let result =
                ArtifactStateMachine::transition(ArtifactStatus::Failed, ArtifactEvent::Retry);
            assert_eq!(result, Ok(ArtifactStatus::Pending));
        }

        #[test]
        fn test_ready_is_terminal() {
            let result =
                ArtifactStateMachine::transition(ArtifactStatus::Ready, ArtifactEvent::Fail);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_ready_cannot_retry() {
            let result =
                ArtifactStateMachine::transition(ArtifactStatus::Ready, ArtifactEvent::Retry);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_pending_cannot_retry() {
            let result =
                ArtifactStateMachine::transition(ArtifactStatus::Pending, ArtifactEvent::Retry);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_failed_cannot_complete() {
            let result =
                ArtifactStateMachine::transition(ArtifactStatus::Failed, ArtifactEvent::Complete);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_is_terminal() {
            assert!(!ArtifactStatus::Pending.is_terminal());
            assert!(ArtifactStatus::Ready.is_terminal());
            assert!(!ArtifactStatus::Failed.is_terminal());
        }

        #[test]
        fn test_valid_transitions() {
            let pending = ArtifactStatus::Pending.valid_transitions();
            assert_eq!(pending.len(), 2);
            assert!(pending.contains(&ArtifactStatus::Ready));
            assert!(pending.contains(&ArtifactStatus::Failed));

            assert!(ArtifactStatus::Ready.valid_transitions().is_empty());

            let failed = ArtifactStatus::Failed.valid_transitions();
            assert_eq!(failed.len(), 1);
            assert!(failed.contains(&ArtifactStatus::Pending));
        }

        #[test]
        fn test_can_transition() {
            assert!(ArtifactStateMachine::can_transition(
                ArtifactStatus::Pending,
                &ArtifactEvent::Complete
            ));
            assert!(ArtifactStateMachine::can_transition(
                ArtifactStatus::Pending,
                &ArtifactEvent::Fail
            ));
            assert!(!ArtifactStateMachine::can_transition(
                ArtifactStatus::Pending,
                &ArtifactEvent::Retry
            ));
            assert!(!ArtifactStateMachine::can_transition(
                ArtifactStatus::Ready,
                &ArtifactEvent::Complete
            ));
            assert!(ArtifactStateMachine::can_transition(
                ArtifactStatus::Failed,
                &ArtifactEvent::Retry
            ));
            assert!(!ArtifactStateMachine::can_transition(
                ArtifactStatus::Failed,
                &ArtifactEvent::Complete
            ));
        }

        #[test]
        fn test_status_display() {
            assert_eq!(ArtifactStatus::Pending.to_string(), "pending");
            assert_eq!(ArtifactStatus::Ready.to_string(), "ready");
            assert_eq!(ArtifactStatus::Failed.to_string(), "failed");
        }

        #[test]
        fn test_event_display() {
            assert_eq!(ArtifactEvent::Complete.to_string(), "complete");
            assert_eq!(ArtifactEvent::Fail.to_string(), "fail");
            assert_eq!(ArtifactEvent::Retry.to_string(), "retry");
        }
    }
}
