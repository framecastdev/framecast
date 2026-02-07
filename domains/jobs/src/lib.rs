//! Jobs domain: jobs, job events, usage

pub mod domain;

// Re-export domain types at the crate root for convenience
pub use domain::entities::*;
pub use domain::state::{JobEvent, JobState, JobStateMachine, StateError};
