//! State machines for teams domain entities
//!
//! This module implements formal state machines as defined in the specification
//! (docs/spec/05_Relationships_States.md). Each state machine defines:
//! - Valid states
//! - Events that trigger transitions
//! - Guard conditions for transitions
//! - Terminal states

pub use framecast_common::StateError;
use serde::{Deserialize, Serialize};

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

    /// Attempt a state transition with guard conditions
    pub fn transition(
        self,
        event: InvitationEvent,
        context: Option<&InvitationGuardContext>,
    ) -> Result<InvitationState, StateError> {
        // Check for terminal state
        if self.is_terminal() {
            return Err(StateError::TerminalState(self.to_string()));
        }

        let next = match (&self, &event) {
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
                    from: self.to_string(),
                    to: "unknown".to_string(),
                    event: event.to_string(),
                });
            }
        };

        Ok(next)
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition(
        self,
        event: &InvitationEvent,
        context: Option<&InvitationGuardContext>,
    ) -> bool {
        self.transition(*event, context).is_ok()
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
#[derive(Debug, Clone, Copy, PartialEq)]
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Invitation State Machine Tests
    // ------------------------------------------------------------------------

    mod invitation_state_machine {
        use super::*;

        #[test]
        fn test_valid_pending_to_accepted() {
            let ctx = InvitationGuardContext { is_expired: false };
            let result = InvitationState::Pending.transition(InvitationEvent::Accept, Some(&ctx));
            assert_eq!(result, Ok(InvitationState::Accepted));
        }

        #[test]
        fn test_valid_pending_to_expired() {
            let result = InvitationState::Pending.transition(InvitationEvent::Expire, None);
            assert_eq!(result, Ok(InvitationState::Expired));
        }

        #[test]
        fn test_valid_pending_to_revoked() {
            let result = InvitationState::Pending.transition(InvitationEvent::Revoke, None);
            assert_eq!(result, Ok(InvitationState::Revoked));
        }

        #[test]
        fn test_guard_fails_accept_expired_invitation() {
            let ctx = InvitationGuardContext { is_expired: true };
            let result = InvitationState::Pending.transition(InvitationEvent::Accept, Some(&ctx));
            assert!(matches!(result, Err(StateError::GuardFailed(_))));
        }

        #[test]
        fn test_terminal_accepted_cannot_transition() {
            let result = InvitationState::Accepted.transition(InvitationEvent::Revoke, None);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_expired_cannot_transition() {
            let result = InvitationState::Expired.transition(InvitationEvent::Accept, None);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_terminal_revoked_cannot_transition() {
            let result = InvitationState::Revoked.transition(InvitationEvent::Accept, None);
            assert!(matches!(result, Err(StateError::TerminalState(_))));
        }

        #[test]
        fn test_valid_pending_to_declined() {
            let result = InvitationState::Pending.transition(InvitationEvent::Decline, None);
            assert_eq!(result, Ok(InvitationState::Declined));
        }

        #[test]
        fn test_terminal_declined_cannot_transition() {
            let result = InvitationState::Declined.transition(InvitationEvent::Accept, None);
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
            // Kill mutant: InvitationState::can_transition -> true / false
            let ctx = InvitationGuardContext { is_expired: false };

            // Valid transitions
            assert!(InvitationState::Pending.can_transition(&InvitationEvent::Accept, Some(&ctx)));
            assert!(InvitationState::Pending.can_transition(&InvitationEvent::Decline, None));
            assert!(InvitationState::Pending.can_transition(&InvitationEvent::Revoke, None));

            // Invalid transitions (from terminal states)
            assert!(!InvitationState::Accepted.can_transition(&InvitationEvent::Revoke, None));
            assert!(!InvitationState::Declined.can_transition(&InvitationEvent::Accept, None));
            assert!(!InvitationState::Expired.can_transition(&InvitationEvent::Accept, None));
        }
    }
}
