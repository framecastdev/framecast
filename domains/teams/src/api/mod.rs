//! API layer for the Teams domain
//!
//! Contains HTTP handlers, routes, authentication middleware, and request/response types.

pub mod handlers;
pub mod middleware;
pub mod routes;

pub use middleware::{ApiKeyUser, AuthConfig, AuthError, AuthUser, SupabaseClaims, TeamsState};
pub use routes::routes;
