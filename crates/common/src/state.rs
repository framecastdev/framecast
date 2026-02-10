//! Common state machine error types
//!
//! Shared across all domain crates that implement state machines.

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
