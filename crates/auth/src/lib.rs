//! Authentication middleware for Framecast API
//!
//! Provides JWT validation, API key verification, and axum extractors
//! that work with any domain state implementing `FromRef<S>` for `AuthBackend`.

mod backend;
mod claims;
mod config;
mod context;
mod error;
mod extractors;
mod jwt;
mod types;

pub use backend::AuthBackend;
pub use claims::SupabaseClaims;
pub use config::AuthConfig;
pub use context::AuthContext;
pub use error::AuthError;
pub use extractors::{AnyAuth, ApiKeyUser, AuthUser, CreatorUser};
pub use types::{AuthApiKey, AuthIdentity, AuthMembership, AuthRole, AuthTier};
