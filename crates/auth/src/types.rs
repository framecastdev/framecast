//! Auth CQRS read-model types
//!
//! Lightweight views of the same DB rows owned by the teams domain.
//! These types carry only the fields needed for authentication and authorization.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Lightweight identity for authenticated users.
///
/// Contains the fields needed by auth middleware and most handlers.
/// Handlers needing full `User` data (credits, storage) should load
/// from their domain's repository.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthIdentity {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub tier: AuthTier,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User tier for auth decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize)]
#[sqlx(type_name = "user_tier", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AuthTier {
    Starter,
    Creator,
}

impl std::fmt::Display for AuthTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthTier::Starter => write!(f, "starter"),
            AuthTier::Creator => write!(f, "creator"),
        }
    }
}

/// Team membership info for authorization checks
#[derive(Debug, Clone)]
pub struct AuthMembership {
    pub team_id: Uuid,
    pub team_name: String,
    pub team_slug: String,
    pub role: AuthRole,
}

/// Membership role for auth decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize)]
#[sqlx(type_name = "membership_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AuthRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl AuthRole {
    /// Check if this role can perform admin actions
    pub fn can_admin(&self) -> bool {
        matches!(self, AuthRole::Owner | AuthRole::Admin)
    }

    /// Check if this role is owner
    pub fn is_owner(&self) -> bool {
        matches!(self, AuthRole::Owner)
    }
}

/// Authenticated API key — excludes sensitive `key_hash` field.
#[derive(Debug, Clone)]
pub struct AuthApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub owner: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
