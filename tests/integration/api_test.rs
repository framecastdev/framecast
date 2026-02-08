//! API endpoint integration tests
//!
//! Tests for all teams-domain API endpoints: users, teams, memberships, api_keys, auth, invariants.

mod api_keys;
mod auth;
#[allow(dead_code)]
mod common;
mod invariants;
mod memberships;
mod teams;
mod users;
