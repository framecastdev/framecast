//! HTTP handlers for Generations domain API

pub mod callbacks;
pub mod generations;
#[cfg(feature = "mock-render")]
pub mod mock_admin;
