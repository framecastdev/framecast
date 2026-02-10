//! State machines for Framecast webhook entities
//!
//! This module implements formal state machines as defined in the specification
//! (docs/spec/05_Relationships_States.md). Each state machine defines:
//! - Valid states
//! - Events that trigger transitions
//! - Guard conditions for transitions
//! - Terminal states

pub use framecast_common::StateError;

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
#[derive(Debug, Clone, Copy, PartialEq)]
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
        Self::transition(current, *event, context).is_ok()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
