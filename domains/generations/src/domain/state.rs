//! State machines for Generations domain entities
//!
//! This module implements formal state machines as defined in the specification
//! (docs/spec/05_Relationships_States.md). Each state machine defines:
//! - Valid states
//! - Events that trigger transitions
//! - Guard conditions for transitions
//! - Terminal states

pub use framecast_common::StateError;

// ============================================================================
// Generation State Machine
// ============================================================================

/// Generation status states as defined in spec section 6.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenerationState {
    Queued,
    Processing,
    Completed,
    Failed,
    Canceled,
}

impl GenerationState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [GenerationState] {
        match self {
            Self::Queued => &[Self::Processing, Self::Canceled],
            Self::Processing => &[Self::Completed, Self::Failed, Self::Canceled],
            Self::Completed => &[],
            Self::Failed => &[],
            Self::Canceled => &[],
        }
    }
}

impl std::fmt::Display for GenerationState {
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

/// Events that trigger generation state transitions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenerationEvent {
    /// Worker picks up the generation for processing
    WorkerPicksUp,
    /// Generation completes successfully
    Success,
    /// Generation fails with an error
    Failure,
    /// Generation is canceled by user or system
    Cancel,
}

impl std::fmt::Display for GenerationEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkerPicksUp => write!(f, "worker_picks_up"),
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::Cancel => write!(f, "cancel"),
        }
    }
}

/// Generation state machine
pub struct GenerationStateMachine;

impl GenerationStateMachine {
    /// Attempt a state transition
    ///
    /// Returns the new state if the transition is valid, or an error otherwise.
    pub fn transition(
        current: GenerationState,
        event: GenerationEvent,
    ) -> Result<GenerationState, StateError> {
        // Check for terminal state
        if current.is_terminal() {
            return Err(StateError::TerminalState(current.to_string()));
        }

        let next = match (&current, &event) {
            // From Queued
            (GenerationState::Queued, GenerationEvent::WorkerPicksUp) => {
                GenerationState::Processing
            }
            (GenerationState::Queued, GenerationEvent::Cancel) => GenerationState::Canceled,

            // From Processing
            (GenerationState::Processing, GenerationEvent::Success) => GenerationState::Completed,
            (GenerationState::Processing, GenerationEvent::Failure) => GenerationState::Failed,
            (GenerationState::Processing, GenerationEvent::Cancel) => GenerationState::Canceled,

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
    pub fn can_transition(current: GenerationState, event: &GenerationEvent) -> bool {
        Self::transition(current, *event).is_ok()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Generation State Machine Tests
    // ------------------------------------------------------------------------

    mod generation_state_machine {
        use super::*;

        #[test]
        fn test_valid_queued_to_processing() {
            let result = GenerationStateMachine::transition(
                GenerationState::Queued,
                GenerationEvent::WorkerPicksUp,
            );
            assert_eq!(result, Ok(GenerationState::Processing));
        }

        #[test]
        fn test_valid_queued_to_canceled() {
            let result = GenerationStateMachine::transition(
                GenerationState::Queued,
                GenerationEvent::Cancel,
            );
            assert_eq!(result, Ok(GenerationState::Canceled));
        }

        #[test]
        fn test_valid_processing_to_completed() {
            let result = GenerationStateMachine::transition(
                GenerationState::Processing,
                GenerationEvent::Success,
            );
            assert_eq!(result, Ok(GenerationState::Completed));
        }

        #[test]
        fn test_valid_processing_to_failed() {
            let result = GenerationStateMachine::transition(
                GenerationState::Processing,
                GenerationEvent::Failure,
            );
            assert_eq!(result, Ok(GenerationState::Failed));
        }

        #[test]
        fn test_valid_processing_to_canceled() {
            let result = GenerationStateMachine::transition(
                GenerationState::Processing,
                GenerationEvent::Cancel,
            );
            assert_eq!(result, Ok(GenerationState::Canceled));
        }

        #[test]
        fn test_invalid_queued_to_completed() {
            let result = GenerationStateMachine::transition(
                GenerationState::Queued,
                GenerationEvent::Success,
            );
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_invalid_queued_to_failed() {
            let result = GenerationStateMachine::transition(
                GenerationState::Queued,
                GenerationEvent::Failure,
            );
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_terminal_completed_cannot_transition() {
            let result = GenerationStateMachine::transition(
                GenerationState::Completed,
                GenerationEvent::Cancel,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_failed_cannot_transition() {
            let result = GenerationStateMachine::transition(
                GenerationState::Failed,
                GenerationEvent::Success,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_canceled_cannot_transition() {
            let result = GenerationStateMachine::transition(
                GenerationState::Canceled,
                GenerationEvent::WorkerPicksUp,
            );
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_is_terminal() {
            assert!(!GenerationState::Queued.is_terminal());
            assert!(!GenerationState::Processing.is_terminal());
            assert!(GenerationState::Completed.is_terminal());
            assert!(GenerationState::Failed.is_terminal());
            assert!(GenerationState::Canceled.is_terminal());
        }

        #[test]
        fn test_can_transition() {
            assert!(GenerationStateMachine::can_transition(
                GenerationState::Queued,
                &GenerationEvent::WorkerPicksUp
            ));
            assert!(!GenerationStateMachine::can_transition(
                GenerationState::Queued,
                &GenerationEvent::Success
            ));
            assert!(!GenerationStateMachine::can_transition(
                GenerationState::Completed,
                &GenerationEvent::Cancel
            ));
        }

        #[test]
        fn test_valid_transitions_from_queued() {
            let transitions = GenerationState::Queued.valid_transitions();
            assert!(transitions.contains(&GenerationState::Processing));
            assert!(transitions.contains(&GenerationState::Canceled));
            assert_eq!(transitions.len(), 2);
        }

        #[test]
        fn test_valid_transitions_from_processing() {
            let transitions = GenerationState::Processing.valid_transitions();
            assert!(transitions.contains(&GenerationState::Completed));
            assert!(transitions.contains(&GenerationState::Failed));
            assert!(transitions.contains(&GenerationState::Canceled));
            assert_eq!(transitions.len(), 3);
        }

        #[test]
        fn test_terminal_states_have_no_transitions() {
            assert!(GenerationState::Completed.valid_transitions().is_empty());
            assert!(GenerationState::Failed.valid_transitions().is_empty());
            assert!(GenerationState::Canceled.valid_transitions().is_empty());
        }
    }
}
