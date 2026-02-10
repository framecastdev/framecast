//! HTTP handlers for Jobs domain API

pub mod callbacks;
pub mod jobs;
#[cfg(feature = "mock-render")]
pub mod mock_admin;
