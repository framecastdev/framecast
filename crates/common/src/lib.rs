//! Shared utilities, configuration, and error handling for Framecast
//!
//! This crate provides common functionality used across the Framecast application:
//! - Configuration management following 12-factor principles
//! - Error types and handling
//! - Utility functions and types
//! - Authentication utilities

pub mod crypto;
pub mod db;
pub mod error;
pub mod extractors;
pub mod state;
pub mod urn;

pub use crypto::{compute_hash_prefix, verify_key_hash};
pub use db::RepositoryError;
pub use error::{Error, Result};
pub use extractors::{Pagination, ValidatedJson};
pub use state::StateError;
pub use urn::{Urn, UrnComponents};
