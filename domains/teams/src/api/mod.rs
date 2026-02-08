//! API layer for the Teams domain
//!
//! Contains HTTP handlers, routes, authentication middleware, and request/response types.

pub mod handlers;
pub mod middleware;
pub mod routes;

pub use middleware::{
    AnyAuth, ApiKeyUser, AuthConfig, AuthError, AuthUser, CreatorUser, SupabaseClaims, TeamsState,
};
pub use routes::routes;
