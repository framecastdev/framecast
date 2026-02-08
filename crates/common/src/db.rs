//! Shared database types for Framecast
//!
//! This module provides common database-related types used across domain repositories.

use crate::error::Error;
use thiserror::Error;

/// Database-specific error types
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Record not found")]
    NotFound,

    #[error("Record already exists")]
    AlreadyExists,

    #[error("Database connection error: {0}")]
    Connection(#[from] sqlx::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<RepositoryError> for Error {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound => Error::NotFound("Record not found".to_string()),
            RepositoryError::AlreadyExists => Error::Conflict("Record already exists".to_string()),
            RepositoryError::Connection(e) => Error::Database(e),
            RepositoryError::InvalidData(msg) => Error::Validation(msg),
        }
    }
}
