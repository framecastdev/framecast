//! API endpoint integration tests
//!
//! Tests for all domain API endpoints: teams, artifacts, conversations, messages.

mod api_keys;
mod artifacts;
mod auth;
#[allow(dead_code)]
mod common;
mod conversations;
mod invariants;
mod jobs;
mod memberships;
mod messages;
mod system_assets;
mod teams;
mod users;
