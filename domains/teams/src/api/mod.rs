//! API layer for the Teams domain
//!
//! Contains HTTP handlers, routes, and domain state definition.

pub mod handlers;
pub mod middleware;
pub mod routes;

pub use middleware::TeamsState;
pub use routes::routes;
