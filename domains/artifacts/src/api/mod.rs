//! API layer for the Artifacts domain
//!
//! Contains HTTP handlers, routes, and domain state definition.

pub mod handlers;
pub mod middleware;
pub mod routes;

pub use middleware::ArtifactsState;
pub use routes::routes;
