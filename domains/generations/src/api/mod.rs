//! API layer for the Generations domain
//!
//! Contains HTTP handlers, routes, and domain state definition.

pub mod handlers;
pub mod middleware;
pub mod routes;

pub use middleware::GenerationsState;
pub use routes::routes;
