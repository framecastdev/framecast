//! Shared utilities, configuration, and error handling for Framecast
//!
//! This crate provides common functionality used across the Framecast application:
//! - Configuration management following 12-factor principles
//! - Error types and handling
//! - Utility functions and types
//! - Authentication utilities

pub mod config;
pub mod error;
pub mod urn;

pub use config::Config;
pub use error::{Error, Result};
pub use urn::{Urn, UrnComponents};
