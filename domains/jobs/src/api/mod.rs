//! API layer for the Jobs domain
//!
//! Contains HTTP handlers, routes, and domain state definition.

pub mod handlers;
pub mod middleware;
pub mod routes;

pub use middleware::JobsState;
pub use routes::routes;
