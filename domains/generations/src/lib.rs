//! Generations domain: AI content generation instances, events, usage

pub mod api;
pub mod domain;
pub mod repository;

// Re-export domain types at the crate root for convenience
pub use domain::entities::*;
pub use domain::state::{GenerationEvent, GenerationState, GenerationStateMachine, StateError};

// Re-export repository types
pub use repository::GenerationsRepositories;

// Re-export API types
pub use api::routes;
pub use api::GenerationsState;
