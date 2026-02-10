//! State machine for conversation status transitions
//!
//! Conversation states: Active ↔ Archived (bidirectional)

pub use framecast_common::StateError;
use serde::{Deserialize, Serialize};

/// Conversation states as defined in spec section 6.6
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationState {
    Active,
    Archived,
}

impl ConversationState {
    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [ConversationState] {
        match self {
            Self::Active => &[Self::Archived],
            Self::Archived => &[Self::Active],
        }
    }
}

impl std::fmt::Display for ConversationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Archived => write!(f, "archived"),
        }
    }
}

/// Events that trigger conversation state transitions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConversationEvent {
    /// Archive the conversation
    Archive,
    /// Unarchive (reactivate) the conversation
    Unarchive,
}

impl std::fmt::Display for ConversationEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archive => write!(f, "archive"),
            Self::Unarchive => write!(f, "unarchive"),
        }
    }
}

/// Conversation state machine
pub struct ConversationStateMachine;

impl ConversationStateMachine {
    /// Attempt a state transition
    pub fn transition(
        current: ConversationState,
        event: ConversationEvent,
    ) -> Result<ConversationState, StateError> {
        let next = match (&current, &event) {
            (ConversationState::Active, ConversationEvent::Archive) => ConversationState::Archived,
            (ConversationState::Archived, ConversationEvent::Unarchive) => {
                ConversationState::Active
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1.3 Conversation State Machine (CON-U18 through CON-U23)

    #[test]
    fn test_active_to_archived() {
        let result = ConversationStateMachine::transition(
            ConversationState::Active,
            ConversationEvent::Archive,
        );
        assert_eq!(result, Ok(ConversationState::Archived));
    }

    #[test]
    fn test_archived_to_active() {
        let result = ConversationStateMachine::transition(
            ConversationState::Archived,
            ConversationEvent::Unarchive,
        );
        assert_eq!(result, Ok(ConversationState::Active));
    }

    #[test]
    fn test_active_cannot_unarchive() {
        let result = ConversationStateMachine::transition(
            ConversationState::Active,
            ConversationEvent::Unarchive,
        );
        assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
    }

    #[test]
    fn test_archived_cannot_archive() {
        let result = ConversationStateMachine::transition(
            ConversationState::Archived,
            ConversationEvent::Archive,
        );
        assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
    }

    #[test]
    fn test_active_valid_transitions() {
        let transitions = ConversationState::Active.valid_transitions();
        assert_eq!(transitions.len(), 1);
        assert!(transitions.contains(&ConversationState::Archived));
    }

    #[test]
    fn test_archived_valid_transitions() {
        let transitions = ConversationState::Archived.valid_transitions();
        assert_eq!(transitions.len(), 1);
        assert!(transitions.contains(&ConversationState::Active));
    }
}
