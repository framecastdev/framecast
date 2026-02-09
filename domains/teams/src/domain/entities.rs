//! Domain entities for Framecast teams domain
//!
//! This module contains teams-related domain entities as defined in the API specification.
//! Each entity includes proper validation, serialization, and business rules.

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use std::collections::HashMap;
use uuid::Uuid;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use framecast_common::{Error, Result, Urn};
use validator::ValidateEmail;

pub use crate::domain::state::InvitationState;
use crate::domain::state::{
    InvitationEvent, InvitationGuardContext, InvitationStateMachine, StateError,
};

/// Maximum number of team memberships a single user can hold (INV-T8)
pub const MAX_TEAM_MEMBERSHIPS: i64 = 50;

/// Maximum number of teams a single user can own (INV-T7)
pub const MAX_OWNED_TEAMS: i64 = 10;

/// User tier levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "user_tier", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum UserTier {
    #[default]
    Starter,
    Creator,
}

impl std::fmt::Display for UserTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserTier::Starter => write!(f, "starter"),
            UserTier::Creator => write!(f, "creator"),
        }
    }
}

/// User entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub tier: UserTier,
    pub credits: i32,
    pub ephemeral_storage_bytes: i64,
    pub upgraded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Create a new user with validation
    pub fn new(id: Uuid, email: String, name: Option<String>) -> Result<Self> {
        // Validate email format (validator crate enforces RFC 5321 including length)
        if !email.validate_email() {
            return Err(Error::Validation("Invalid email format".to_string()));
        }

        // Validate name length if provided
        if let Some(ref name) = name {
            if name.is_empty() || name.len() > 100 {
                return Err(Error::Validation(
                    "Name must be 1-100 characters".to_string(),
                ));
            }
        }

        let now = Utc::now();
        Ok(User {
            id,
            email,
            name,
            avatar_url: None,
            tier: UserTier::default(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Upgrade user to creator tier
    pub fn upgrade_to_creator(&mut self) -> Result<()> {
        if self.tier == UserTier::Creator {
            return Err(Error::Validation("User is already a creator".to_string()));
        }

        self.tier = UserTier::Creator;
        self.upgraded_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // INV-U1: Creator users have upgrade timestamp
        if self.tier == UserTier::Creator && self.upgraded_at.is_none() {
            return Err(Error::Validation(
                "Creator users must have upgrade timestamp".to_string(),
            ));
        }

        // INV-U5: Credits cannot be negative
        if self.credits < 0 {
            return Err(Error::Validation("Credits cannot be negative".to_string()));
        }

        // INV-U6: Storage cannot be negative
        if self.ephemeral_storage_bytes < 0 {
            return Err(Error::Validation("Storage cannot be negative".to_string()));
        }

        // Email validation (validator crate enforces RFC 5321 including length)
        if !self.email.validate_email() {
            return Err(Error::Validation("Invalid email format".to_string()));
        }

        // Name validation
        if let Some(ref name) = self.name {
            if name.is_empty() || name.len() > 100 {
                return Err(Error::Validation(
                    "Name must be 1-100 characters".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check if user can create teams (must be creator)
    pub fn can_create_teams(&self) -> bool {
        self.tier == UserTier::Creator
    }

    /// Generate user URN
    pub fn urn(&self) -> Urn {
        Urn::user(self.id)
    }
}

/// Team entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub credits: i32,
    pub ephemeral_storage_bytes: i64,
    pub settings: Json<HashMap<String, serde_json::Value>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Team {
    /// Create a new team with validation
    pub fn new(name: String, slug: Option<String>) -> Result<Self> {
        // Validate name
        if name.is_empty() || name.len() > 100 {
            return Err(Error::Validation(
                "Team name must be 1-100 characters".to_string(),
            ));
        }

        // Generate or validate slug
        let slug = match slug {
            Some(s) => {
                Self::validate_slug(&s)?;
                s
            }
            None => Self::generate_slug(&name)?,
        };

        let now = Utc::now();
        Ok(Team {
            id: Uuid::new_v4(),
            name,
            slug,
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        })
    }

    /// Validate slug format per INV-T4
    pub fn validate_slug(slug: &str) -> Result<()> {
        if slug.is_empty() || slug.len() > 50 {
            return Err(Error::Validation(
                "Slug must be 1-50 characters".to_string(),
            ));
        }

        // Check format: lowercase alphanumeric + hyphens, no leading/trailing hyphen
        if !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(Error::Validation(
                "Slug must contain only lowercase letters, numbers, and hyphens".to_string(),
            ));
        }

        if slug.starts_with('-') || slug.ends_with('-') {
            return Err(Error::Validation(
                "Slug cannot start or end with a hyphen".to_string(),
            ));
        }

        if slug.contains("--") {
            return Err(Error::Validation(
                "Slug cannot contain consecutive hyphens".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate slug from name with random suffix if needed
    fn generate_slug(name: &str) -> Result<String> {
        let raw = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>();

        // Collapse consecutive hyphens and trim leading/trailing
        let mut base = String::with_capacity(raw.len());
        let mut prev_hyphen = false;
        for ch in raw.chars() {
            if ch == '-' {
                if !prev_hyphen {
                    base.push(ch);
                }
                prev_hyphen = true;
            } else {
                base.push(ch);
                prev_hyphen = false;
            }
        }
        let base = base.trim_matches('-').to_string();

        if base.is_empty() {
            return Err(Error::Validation(
                "Cannot generate valid slug from name".to_string(),
            ));
        }

        // Add random suffix to ensure uniqueness
        let suffix = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let slug = format!("{}-{}", base, suffix);

        Self::validate_slug(&slug)?;
        Ok(slug)
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Name validation
        if self.name.is_empty() || self.name.len() > 100 {
            return Err(Error::Validation(
                "Team name must be 1-100 characters".to_string(),
            ));
        }

        // Slug validation
        Self::validate_slug(&self.slug)?;

        // INV-T6: Team credits cannot be negative
        if self.credits < 0 {
            return Err(Error::Validation(
                "Team credits cannot be negative".to_string(),
            ));
        }

        // Storage cannot be negative
        if self.ephemeral_storage_bytes < 0 {
            return Err(Error::Validation(
                "Team storage cannot be negative".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate team URN
    pub fn urn(&self) -> Urn {
        Urn::team(self.id)
    }
}

/// Membership roles within a team
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "membership_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MembershipRole {
    Owner,
    Admin,
    #[default]
    Member,
    Viewer,
}

impl std::fmt::Display for MembershipRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MembershipRole::Owner => write!(f, "owner"),
            MembershipRole::Admin => write!(f, "admin"),
            MembershipRole::Member => write!(f, "member"),
            MembershipRole::Viewer => write!(f, "viewer"),
        }
    }
}

impl MembershipRole {
    /// Check if this role can perform admin actions
    pub fn can_admin(&self) -> bool {
        matches!(self, MembershipRole::Owner | MembershipRole::Admin)
    }

    /// Check if this role is owner
    pub fn is_owner(&self) -> bool {
        matches!(self, MembershipRole::Owner)
    }

    /// Check if this role can invite users
    pub fn can_invite(&self) -> bool {
        self.can_admin()
    }

    /// Check if this role can modify team settings
    pub fn can_modify_team(&self) -> bool {
        self.can_admin()
    }
}

/// Role for invitation (excludes Owner since owners cannot be invited)
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "invitation_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum InvitationRole {
    Admin,
    #[default]
    Member,
    Viewer,
}

impl InvitationRole {
    /// Convert to MembershipRole for use after invitation is accepted
    pub fn to_membership_role(&self) -> MembershipRole {
        match self {
            InvitationRole::Admin => MembershipRole::Admin,
            InvitationRole::Member => MembershipRole::Member,
            InvitationRole::Viewer => MembershipRole::Viewer,
        }
    }
}

impl TryFrom<MembershipRole> for InvitationRole {
    type Error = Error;

    fn try_from(role: MembershipRole) -> Result<Self> {
        match role {
            MembershipRole::Admin => Ok(InvitationRole::Admin),
            MembershipRole::Member => Ok(InvitationRole::Member),
            MembershipRole::Viewer => Ok(InvitationRole::Viewer),
            MembershipRole::Owner => Err(Error::Validation(
                "Cannot invite owners via invitation".to_string(),
            )),
        }
    }
}

impl std::fmt::Display for InvitationRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvitationRole::Admin => write!(f, "admin"),
            InvitationRole::Member => write!(f, "member"),
            InvitationRole::Viewer => write!(f, "viewer"),
        }
    }
}

/// Membership entity - association between User and Team
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Membership {
    pub id: Uuid,
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: MembershipRole,
    pub created_at: DateTime<Utc>,
}

impl Membership {
    /// Create a new membership with validation
    pub fn new(team_id: Uuid, user_id: Uuid, role: MembershipRole) -> Self {
        Membership {
            id: Uuid::new_v4(),
            team_id,
            user_id,
            role,
            created_at: Utc::now(),
        }
    }
}

/// Invitation entity - pending invitation to join a team
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub team_id: Uuid,
    pub invited_by: Uuid,
    pub email: String,
    pub role: InvitationRole, // Cannot be Owner per constraints
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Invitation {
    /// Create a new invitation with validation
    pub fn new(
        team_id: Uuid,
        invited_by: Uuid,
        email: String,
        role: InvitationRole,
    ) -> Result<Self> {
        // Validate email
        if !email.validate_email() {
            return Err(Error::Validation("Invalid email format".to_string()));
        }

        // Generate secure token: 32 random bytes, URL-safe base64 encoded (43 chars)
        let mut token_bytes = [0u8; 32];
        getrandom::getrandom(&mut token_bytes)
            .map_err(|e| Error::Internal(format!("Failed to generate random bytes: {}", e)))?;
        let token = URL_SAFE_NO_PAD.encode(token_bytes);

        let now = Utc::now();
        Ok(Invitation {
            id: Uuid::new_v4(),
            team_id,
            invited_by,
            email,
            role,
            token,
            expires_at: now + chrono::Duration::days(7),
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now,
        })
    }

    /// Get current invitation state
    pub fn state(&self) -> InvitationState {
        if self.accepted_at.is_some() {
            InvitationState::Accepted
        } else if self.declined_at.is_some() {
            InvitationState::Declined
        } else if self.revoked_at.is_some() {
            InvitationState::Revoked
        } else if self.expires_at < Utc::now() {
            InvitationState::Expired
        } else {
            InvitationState::Pending
        }
    }

    /// Check if invitation can be acted upon
    pub fn is_actionable(&self) -> bool {
        !self.state().is_terminal()
    }

    /// Check if invitation is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }

    /// Accept the invitation
    pub fn accept(&mut self) -> Result<()> {
        self.apply_transition(InvitationEvent::Accept)?;
        self.accepted_at = Some(Utc::now());
        Ok(())
    }

    /// Decline the invitation (invitee-initiated)
    pub fn decline(&mut self) -> Result<()> {
        self.apply_transition(InvitationEvent::Decline)?;
        self.declined_at = Some(Utc::now());
        Ok(())
    }

    /// Revoke the invitation (admin-initiated)
    pub fn revoke(&mut self) -> Result<()> {
        self.apply_transition(InvitationEvent::Revoke)?;
        self.revoked_at = Some(Utc::now());
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: InvitationEvent) -> Result<InvitationState> {
        let current_state = self.state();
        let context = InvitationGuardContext {
            is_expired: self.is_expired(),
        };
        InvitationStateMachine::transition(current_state, event, Some(&context)).map_err(
            |e| match e {
                StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                    "Invalid invitation transition: cannot apply '{}' event from '{}' state",
                    event, from
                )),
                StateError::TerminalState(state) => Error::Validation(format!(
                    "Invitation is in terminal state '{}' and cannot transition",
                    state
                )),
                StateError::GuardFailed(msg) => Error::Validation(msg),
            },
        )
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &InvitationEvent) -> bool {
        let context = InvitationGuardContext {
            is_expired: self.is_expired(),
        };
        InvitationStateMachine::can_transition(self.state(), event, Some(&context))
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Email validation
        if !self.email.validate_email() {
            return Err(Error::Validation("Invalid email format".to_string()));
        }

        // Note: Role validation for Owner is enforced at the type level -
        // InvitationRole doesn't include Owner variant

        // State validation: at most one terminal timestamp can be set
        let terminal_count = [
            self.accepted_at.is_some(),
            self.declined_at.is_some(),
            self.revoked_at.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();
        if terminal_count > 1 {
            return Err(Error::Validation(
                "Invitation cannot have multiple terminal states".to_string(),
            ));
        }

        // Time validation
        if self.created_at >= self.expires_at {
            return Err(Error::Validation(
                "Expiration must be after creation".to_string(),
            ));
        }

        Ok(())
    }
}

/// API Key entity
#[derive(Clone, PartialEq, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub owner: String, // URN as string for database compatibility
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub scopes: Json<Vec<String>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKey")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("owner", &self.owner)
            .field("name", &self.name)
            .field("key_prefix", &self.key_prefix)
            .field("key_hash", &"[REDACTED]")
            .field("scopes", &self.scopes)
            .field("last_used_at", &self.last_used_at)
            .field("expires_at", &self.expires_at)
            .field("revoked_at", &self.revoked_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl ApiKey {
    /// Create a new API key with validation
    ///
    /// Returns `(ApiKey, raw_key)` — the raw key is only available at creation time.
    pub fn new(
        user_id: Uuid,
        owner: Urn,
        name: Option<String>,
        scopes: Option<Vec<String>>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(Self, String)> {
        let name = name.unwrap_or_else(|| "Default".to_string());
        if name.len() > 100 {
            return Err(Error::Validation(
                "Key name must be ≤100 characters".to_string(),
            ));
        }

        let scopes = scopes.unwrap_or_else(|| vec!["*".to_string()]);

        // Generate key components
        let key_prefix = "sk_live_".to_string();
        let full_key = format!(
            "{}{}",
            key_prefix,
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );

        // SECURITY: Use SHA-256 with random salt for production-grade hashing
        let salt: [u8; 32] = rand::thread_rng().gen();
        let key_hash = Self::hash_key(&full_key, &salt);

        let api_key = ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: owner.to_string(),
            name,
            key_prefix,
            key_hash,
            scopes: Json(scopes),
            last_used_at: None,
            expires_at,
            revoked_at: None,
            created_at: Utc::now(),
        };

        Ok((api_key, full_key))
    }

    /// Check if key is valid (not revoked or expired)
    pub fn is_valid(&self) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }

        if let Some(expires_at) = self.expires_at {
            if expires_at < Utc::now() {
                return false;
            }
        }

        true
    }

    /// Revoke the key
    pub fn revoke(&mut self) {
        self.revoked_at = Some(Utc::now());
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Name validation
        if self.name.len() > 100 {
            return Err(Error::Validation(
                "Key name must be ≤100 characters".to_string(),
            ));
        }

        // Hash validation
        if self.key_hash.is_empty() {
            return Err(Error::Validation("Key hash cannot be empty".to_string()));
        }

        // Validate owner URN format
        let _urn = self.owner_urn()?;

        Ok(())
    }

    /// Hash an API key with salt using SHA-256 (production-grade cryptography)
    fn hash_key(key: &str, salt: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(salt);
        let hash = hasher.finalize();

        // Encode as hex string with salt prepended for storage
        format!("{}:{}", hex::encode(salt), hex::encode(hash))
    }

    /// Verify an API key against stored hash using constant-time comparison
    pub fn verify_key(&self, candidate_key: &str) -> bool {
        framecast_common::verify_key_hash(candidate_key, &self.key_hash)
    }
}

/// Authenticated API key — all fields except `key_hash`.
///
/// The repository layer converts `ApiKey → AuthenticatedApiKey` at the boundary
/// so that handlers and middleware never see the sensitive `key_hash` field.
/// This breaks CodeQL's name-based taint chain for `rust/cleartext-logging`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthenticatedApiKey {
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

impl From<ApiKey> for AuthenticatedApiKey {
    fn from(key: ApiKey) -> Self {
        Self {
            id: key.id,
            user_id: key.user_id,
            owner: key.owner,
            name: key.name,
            key_prefix: key.key_prefix,
            scopes: key.scopes.0,
            last_used_at: key.last_used_at,
            expires_at: key.expires_at,
            revoked_at: key.revoked_at,
            created_at: key.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user_id = Uuid::new_v4();
        let email = "test@example.com".to_string();
        let name = Some("Test User".to_string());

        let user = User::new(user_id, email.clone(), name.clone()).unwrap();

        assert_eq!(user.id, user_id);
        assert_eq!(user.email, email);
        assert_eq!(user.name, name);
        assert_eq!(user.tier, UserTier::Starter);
        assert_eq!(user.credits, 0);
        assert!(user.upgraded_at.is_none());
    }

    #[test]
    fn test_user_validation() {
        let user_id = Uuid::new_v4();

        // Test invalid email
        let result = User::new(user_id, "invalid-email".to_string(), None);
        assert!(result.is_err());

        // Test email too long
        let long_email = format!("{}@example.com", "a".repeat(250));
        let result = User::new(user_id, long_email, None);
        assert!(result.is_err());

        // Test name too long
        let result = User::new(
            user_id,
            "test@example.com".to_string(),
            Some("a".repeat(101)),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_user_upgrade_to_creator() {
        let user_id = Uuid::new_v4();
        let mut user = User::new(user_id, "test@example.com".to_string(), None).unwrap();

        assert_eq!(user.tier, UserTier::Starter);
        assert!(user.upgraded_at.is_none());
        assert!(!user.can_create_teams());

        user.upgrade_to_creator().unwrap();

        assert_eq!(user.tier, UserTier::Creator);
        assert!(user.upgraded_at.is_some());
        assert!(user.can_create_teams());

        // Test double upgrade fails
        let result = user.upgrade_to_creator();
        assert!(result.is_err());
    }

    #[test]
    fn test_user_invariants() {
        let user_id = Uuid::new_v4();
        let mut user = User::new(user_id, "test@example.com".to_string(), None).unwrap();

        // Valid starter user
        assert!(user.validate().is_ok());

        // Invalid: creator without upgrade timestamp
        user.tier = UserTier::Creator;
        assert!(user.validate().is_err());

        // Fix it
        user.upgraded_at = Some(Utc::now());
        assert!(user.validate().is_ok());

        // Invalid: negative credits
        user.credits = -1;
        assert!(user.validate().is_err());
    }

    #[test]
    fn test_user_name_empty_rejected() {
        let result = User::new(
            Uuid::new_v4(),
            "test@example.com".to_string(),
            Some("".to_string()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_user_name_whitespace_only_rejected() {
        let result = User::new(
            Uuid::new_v4(),
            "test@example.com".to_string(),
            Some("   ".to_string()),
        );
        // Whitespace-only is allowed (non-empty, within length limit).
        // The name "   " has len 3, which passes the is_empty()/len() check.
        // If we want to reject it, we'd need to add trim validation.
        // Current code allows it — this test documents the behavior.
        assert!(result.is_ok());
    }

    #[test]
    fn test_team_slug_consecutive_hyphens_collapsed_in_generation() {
        // Name with multiple consecutive special chars should collapse hyphens
        let team = Team::new("a---b".to_string(), None).unwrap();
        assert!(team.slug.starts_with("a-b-"));
        assert!(!team.slug.contains("--"));
    }

    #[test]
    fn test_team_creation() {
        let team = Team::new("Test Team".to_string(), None).unwrap();

        assert_eq!(team.name, "Test Team");
        assert!(!team.slug.is_empty());
        assert!(team.slug.contains("test-team"));
        assert_eq!(team.credits, 0);
    }

    #[test]
    fn test_team_slug_validation() {
        // Valid slugs
        assert!(Team::validate_slug("test-team").is_ok());
        assert!(Team::validate_slug("a").is_ok());
        assert!(Team::validate_slug("team123").is_ok());

        // Invalid slugs
        assert!(Team::validate_slug("").is_err());
        assert!(Team::validate_slug("-invalid").is_err());
        assert!(Team::validate_slug("invalid-").is_err());
        assert!(Team::validate_slug("UPPERCASE").is_err());
        assert!(Team::validate_slug("with_underscore").is_err());
        assert!(Team::validate_slug(&"a".repeat(51)).is_err());
    }

    #[test]
    fn test_team_generation_from_name() {
        // Test various team names
        let team1 = Team::new("My Awesome Team!".to_string(), None).unwrap();
        assert!(team1.slug.starts_with("my-awesome-team-"));

        let team2 = Team::new("Special@Characters#Here".to_string(), None).unwrap();
        assert!(team2.slug.starts_with("special-characters-here-"));

        // HTML in name: consecutive special chars collapse to single hyphen
        let team3 = Team::new("<b>Bold Team</b>".to_string(), None).unwrap();
        assert!(team3.slug.starts_with("b-bold-team-b-"));
        assert!(!team3.slug.contains("--"));
    }

    #[test]
    fn test_membership_roles() {
        assert!(MembershipRole::Owner.is_owner());
        assert!(!MembershipRole::Admin.is_owner());

        assert!(MembershipRole::Owner.can_admin());
        assert!(MembershipRole::Admin.can_admin());
        assert!(!MembershipRole::Member.can_admin());
        assert!(!MembershipRole::Viewer.can_admin());

        assert!(MembershipRole::Owner.can_modify_team());
        assert!(MembershipRole::Admin.can_modify_team());
        assert!(!MembershipRole::Member.can_modify_team());
        assert!(!MembershipRole::Viewer.can_modify_team());
    }

    #[test]
    fn test_invitation_creation() {
        let team_id = Uuid::new_v4();
        let invited_by = Uuid::new_v4();
        let email = "invitee@example.com".to_string();
        let role = InvitationRole::Member;

        let invitation = Invitation::new(team_id, invited_by, email.clone(), role).unwrap();

        assert_eq!(invitation.team_id, team_id);
        assert_eq!(invitation.invited_by, invited_by);
        assert_eq!(invitation.email, email);
        assert_eq!(invitation.role, role);
        assert!(!invitation.token.is_empty());
        assert!(invitation.expires_at > Utc::now());
        assert!(invitation.is_actionable());
        assert_eq!(invitation.state(), InvitationState::Pending);
    }

    #[test]
    fn test_invitation_owner_restriction() {
        // Test that MembershipRole::Owner cannot be converted to InvitationRole
        let result = InvitationRole::try_from(MembershipRole::Owner);
        assert!(result.is_err());

        // Test that valid roles convert successfully
        let admin_result = InvitationRole::try_from(MembershipRole::Admin);
        assert!(admin_result.is_ok());
        assert_eq!(admin_result.unwrap(), InvitationRole::Admin);
    }

    #[test]
    fn test_invitation_state_transitions() {
        let team_id = Uuid::new_v4();
        let invited_by = Uuid::new_v4();
        let mut invitation = Invitation::new(
            team_id,
            invited_by,
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        // Test acceptance
        assert!(invitation.is_actionable());
        invitation.accept().unwrap();
        assert_eq!(invitation.state(), InvitationState::Accepted);
        assert!(!invitation.is_actionable());

        // Cannot revoke accepted invitation
        assert!(invitation.revoke().is_err());
    }

    #[test]
    fn test_invitation_revocation() {
        let team_id = Uuid::new_v4();
        let invited_by = Uuid::new_v4();
        let mut invitation = Invitation::new(
            team_id,
            invited_by,
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        invitation.revoke().unwrap();
        assert_eq!(invitation.state(), InvitationState::Revoked);
        assert!(!invitation.is_actionable());
    }

    #[test]
    fn test_api_key_creation() {
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let name = Some("Test Key".to_string());

        let (api_key, raw_key) =
            ApiKey::new(user_id, owner.clone(), name.clone(), None, None).unwrap();

        assert_eq!(api_key.user_id, user_id);
        assert_eq!(api_key.owner_urn().unwrap(), owner);
        assert_eq!(api_key.name, "Test Key");
        assert!(api_key.key_prefix.starts_with("sk_live_"));
        assert!(!api_key.key_hash.is_empty());
        assert!(api_key.is_valid());
        assert!(raw_key.starts_with("sk_live_"));
    }

    #[test]
    fn test_api_key_validation() {
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);

        // Test name too long
        let result = ApiKey::new(user_id, owner.clone(), Some("a".repeat(101)), None, None);
        assert!(result.is_err());

        // Test valid key
        let (api_key, _raw_key) = ApiKey::new(user_id, owner, None, None, None).unwrap();
        assert!(api_key.validate().is_ok());
    }

    #[test]
    fn test_api_key_revocation() {
        let user_id = Uuid::new_v4();
        let (mut api_key, _raw_key) =
            ApiKey::new(user_id, Urn::user(user_id), None, None, None).unwrap();

        assert!(api_key.is_valid());

        api_key.revoke();
        assert!(!api_key.is_valid());
        assert!(api_key.revoked_at.is_some());
    }

    #[test]
    fn test_api_key_new_returns_raw_key() {
        let user_id = Uuid::new_v4();
        let (_, raw_key) = ApiKey::new(user_id, Urn::user(user_id), None, None, None).unwrap();
        assert!(raw_key.starts_with("sk_live_"));
    }

    #[test]
    fn test_api_key_new_raw_key_verifies() {
        let user_id = Uuid::new_v4();
        let (api_key, raw_key) =
            ApiKey::new(user_id, Urn::user(user_id), None, None, None).unwrap();
        assert!(api_key.verify_key(&raw_key));
        assert!(!api_key.verify_key("sk_live_wrong"));
    }

    #[test]
    fn test_api_key_new_with_expires_at() {
        let user_id = Uuid::new_v4();
        let future = Utc::now() + chrono::Duration::days(30);
        let (api_key, _) =
            ApiKey::new(user_id, Urn::user(user_id), None, None, Some(future)).unwrap();
        assert_eq!(api_key.expires_at, Some(future));
    }

    #[test]
    fn test_api_key_new_without_expires_at() {
        let user_id = Uuid::new_v4();
        let (api_key, _) = ApiKey::new(user_id, Urn::user(user_id), None, None, None).unwrap();
        assert!(api_key.expires_at.is_none());
    }

    #[test]
    fn test_api_key_secure_hashing_and_verification() {
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);

        // Create API key with secure hashing
        let (api_key, raw_key) = ApiKey::new(user_id, owner.clone(), None, None, None).unwrap();

        // The hash should be in salt:hash format with hex encoding
        assert!(api_key.key_hash.contains(':'));
        let parts: Vec<&str> = api_key.key_hash.split(':').collect();
        assert_eq!(parts.len(), 2);

        // Both salt and hash should be valid hex
        assert!(hex::decode(parts[0]).is_ok());
        assert!(hex::decode(parts[1]).is_ok());

        // The hash should be 64 characters (SHA-256 = 32 bytes = 64 hex chars)
        assert_eq!(parts[1].len(), 64);

        // Verify raw key works
        assert!(api_key.verify_key(&raw_key));

        // Additional tests with manually constructed keys
        let test_key = "sk_live_test123456789";
        let salt: [u8; 32] = rand::thread_rng().gen();
        let test_hash = ApiKey::hash_key(test_key, &salt);

        // Create a test API key with known hash
        let mut test_api_key = ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: owner.to_string(),
            name: "Test".to_string(),
            key_prefix: "sk_live_".to_string(),
            key_hash: test_hash,
            scopes: sqlx::types::Json(vec!["*".to_string()]),
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        };

        // Test verification with correct key
        assert!(test_api_key.verify_key(test_key));

        // Test verification with wrong key
        assert!(!test_api_key.verify_key("wrong_key"));
        assert!(!test_api_key.verify_key("sk_live_wrong"));

        // Test verification with empty key
        assert!(!test_api_key.verify_key(""));

        // Test verification with malformed hash
        test_api_key.key_hash = "invalid:hash".to_string();
        assert!(!test_api_key.verify_key(test_key));

        // Test verification with missing colon
        test_api_key.key_hash = "invalidhash".to_string();
        assert!(!test_api_key.verify_key(test_key));
    }

    #[test]
    fn test_user_urn_generation() {
        let user_id = Uuid::new_v4();
        let user = User::new(user_id, "test@example.com".to_string(), None).unwrap();
        let urn = user.urn();

        assert_eq!(urn, Urn::user(user_id));
        assert!(urn.is_user());
    }

    #[test]
    fn test_team_urn_generation() {
        let team = Team::new("Test Team".to_string(), None).unwrap();
        let urn = team.urn();

        assert_eq!(urn, Urn::team(team.id));
        assert!(urn.is_team());
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Test that entities can be serialized and deserialized
        let user_id = Uuid::new_v4();
        let user = User::new(
            user_id,
            "test@example.com".to_string(),
            Some("Test".to_string()),
        )
        .unwrap();

        let json = serde_json::to_string(&user).unwrap();
        let deserialized: User = serde_json::from_str(&json).unwrap();

        assert_eq!(user, deserialized);
    }

    // ========================================================================
    // Invitation Declined Edge Cases
    // ========================================================================

    #[test]
    fn test_invitation_decline_sets_state() {
        let mut invitation = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        invitation.decline().unwrap();
        assert_eq!(invitation.state(), InvitationState::Declined);
    }

    #[test]
    fn test_invitation_decline_sets_declined_at() {
        let mut invitation = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        assert!(invitation.declined_at.is_none());
        invitation.decline().unwrap();
        assert!(invitation.declined_at.is_some());
    }

    #[test]
    fn test_invitation_cannot_accept_after_declined() {
        let mut invitation = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        invitation.decline().unwrap();
        let result = invitation.accept();
        assert!(result.is_err());
    }

    #[test]
    fn test_invitation_cannot_revoke_after_declined() {
        let mut invitation = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        invitation.decline().unwrap();
        let result = invitation.revoke();
        assert!(result.is_err());
    }

    #[test]
    fn test_invitation_multiple_terminal_fields_rejected() {
        let mut invitation = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "test@example.com".to_string(),
            InvitationRole::Member,
        )
        .unwrap();

        // Manually set both accepted_at and declined_at (impossible via normal API)
        invitation.accepted_at = Some(Utc::now());
        invitation.declined_at = Some(Utc::now());

        let result = invitation.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_invitation_empty_email_rejected() {
        let result = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "".to_string(),
            InvitationRole::Member,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_invitation_email_valid_format_accepted() {
        let result = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "valid.user+tag@example.com".to_string(),
            InvitationRole::Member,
        );
        assert!(result.is_ok());
    }

    // ========================================================================
    // Team Slug Boundary Tests
    // ========================================================================

    #[test]
    fn test_slug_exactly_max_length_valid() {
        // Slug at max length (50 chars) should be accepted
        let slug = "a".repeat(50);
        assert!(Team::validate_slug(&slug).is_ok());
    }

    #[test]
    fn test_slug_over_max_length_invalid() {
        // Slug at max+1 (51 chars) should be rejected
        let slug = "a".repeat(51);
        assert!(Team::validate_slug(&slug).is_err());
    }

    #[test]
    fn test_slug_single_char_valid() {
        assert!(Team::validate_slug("a").is_ok());
        assert!(Team::validate_slug("z").is_ok());
        assert!(Team::validate_slug("5").is_ok());
    }

    #[test]
    fn test_slug_only_digits_valid() {
        assert!(Team::validate_slug("123").is_ok());
        assert!(Team::validate_slug("007").is_ok());
    }

    #[test]
    fn test_slug_consecutive_hyphens_rejected() {
        assert!(Team::validate_slug("a--b").is_err());
        assert!(Team::validate_slug("a---b").is_err());
    }

    #[test]
    fn test_slug_unicode_rejected() {
        assert!(Team::validate_slug("caf\u{e9}").is_err());
        assert!(Team::validate_slug("\u{65e5}\u{672c}\u{8a9e}").is_err());
        assert!(Team::validate_slug("team-\u{3b1}\u{3b2}").is_err());
    }

    // ========================================================================
    // User Tier Absorbing State
    // ========================================================================

    #[test]
    fn test_user_upgrade_is_one_way() {
        let mut user = User::new(
            Uuid::new_v4(),
            "test@example.com".to_string(),
            Some("Test".to_string()),
        )
        .unwrap();

        user.upgrade_to_creator().unwrap();
        assert_eq!(user.tier, UserTier::Creator);

        // Attempting to upgrade again fails (already creator)
        let result = user.upgrade_to_creator();
        assert!(result.is_err());
    }

    // ========================================================================
    // Security-Oriented Input Tests
    // ========================================================================

    #[test]
    fn test_slug_sql_injection_rejected() {
        assert!(Team::validate_slug("a; DROP TABLE teams").is_err());
        assert!(Team::validate_slug("a' OR '1'='1").is_err());
    }

    #[test]
    fn test_slug_xss_rejected() {
        assert!(Team::validate_slug("<script>alert(1)</script>").is_err());
        assert!(Team::validate_slug("a<img src=x>").is_err());
    }

    #[test]
    fn test_slug_path_traversal_rejected() {
        assert!(Team::validate_slug("../etc/passwd").is_err());
        assert!(Team::validate_slug("..%2f..%2f").is_err());
    }

    // ========================================================================
    // Mutant-killing tests: User::validate
    // ========================================================================

    #[test]
    fn test_user_validate_credits_boundary() {
        // Kill: replace < with > (credits < 0)
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: None,
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };
        // credits = 0 should be valid
        assert!(user.validate().is_ok());
        // credits = -1 should be invalid
        user.credits = -1;
        assert!(user.validate().is_err());
        // credits = 1 should be valid
        user.credits = 1;
        assert!(user.validate().is_ok());
    }

    #[test]
    fn test_user_validate_storage_boundary() {
        // Kill: replace < with > (ephemeral_storage_bytes < 0)
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: None,
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(user.validate().is_ok());
        user.ephemeral_storage_bytes = -1;
        assert!(user.validate().is_err());
    }

    #[test]
    fn test_user_validate_email_format() {
        // Kill: replace validate_email with true/false, delete negation
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "valid@example.com".to_string(),
            name: None,
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };
        // Valid email passes
        assert!(user.validate().is_ok());

        // No domain -> invalid
        user.email = "noemailatall".to_string();
        assert!(user.validate().is_err());

        // Missing local part -> invalid
        user.email = "@example.com".to_string();
        assert!(user.validate().is_err());

        // RFC local-part exceeds 64 chars -> invalid
        user.email = format!("{}@example.com", "a".repeat(65));
        assert!(user.validate().is_err());

        // Bare '@' -> invalid
        user.email = "foo@".to_string();
        assert!(user.validate().is_err());
    }

    #[test]
    fn test_user_validate_name_or_conditions() {
        // Kill: replace || with && in name check
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: Some("".to_string()),
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };
        // Empty name should fail (empty but not > 100)
        assert!(user.validate().is_err());

        // Name > 100 but not empty should fail
        user.name = Some("a".repeat(101));
        assert!(user.validate().is_err());
    }

    #[test]
    fn test_user_validate_name_len_boundary() {
        // Kill: replace > with ==, <, >= (name.len() > 100)
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: Some("a".repeat(100)),
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };
        // 100-char name should be valid
        assert!(user.validate().is_ok());

        // 101-char name should be invalid
        user.name = Some("a".repeat(101));
        assert!(user.validate().is_err());

        // 99-char name should be valid
        user.name = Some("a".repeat(99));
        assert!(user.validate().is_ok());
    }

    // ========================================================================
    // Mutant-killing tests: Team::validate
    // ========================================================================

    #[test]
    fn test_team_validate_returns_err_on_invalid() {
        // Kill: replace Result with Ok(()) (entire validate)
        let now = Utc::now();
        let team = Team {
            id: Uuid::new_v4(),
            name: "".to_string(), // invalid - empty name
            slug: "valid-slug".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        assert!(team.validate().is_err());
    }

    #[test]
    fn test_team_validate_name_or_conditions() {
        // Kill: replace || with && (name check)
        let now = Utc::now();
        // Empty name but not > 100 -> should fail
        let team1 = Team {
            id: Uuid::new_v4(),
            name: "".to_string(),
            slug: "valid-slug".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        assert!(team1.validate().is_err());

        // Name > 100 but not empty -> should fail
        let team2 = Team {
            id: Uuid::new_v4(),
            name: "a".repeat(101),
            slug: "valid-slug".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        assert!(team2.validate().is_err());
    }

    #[test]
    fn test_team_validate_name_len_boundary() {
        // Kill: replace > with ==, <, >= (name.len() > 100)
        let now = Utc::now();
        // 100-char name should be valid
        let team100 = Team {
            id: Uuid::new_v4(),
            name: "a".repeat(100),
            slug: "valid-slug".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        assert!(team100.validate().is_ok());

        // 101-char name should be invalid
        let team101 = Team {
            id: Uuid::new_v4(),
            name: "a".repeat(101),
            slug: "valid-slug".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        assert!(team101.validate().is_err());

        // 99-char name should be valid
        let team99 = Team {
            id: Uuid::new_v4(),
            name: "a".repeat(99),
            slug: "valid-slug".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        assert!(team99.validate().is_ok());
    }

    #[test]
    fn test_team_validate_credits_boundary() {
        // Kill: replace < with ==, >, <= (credits < 0)
        let now = Utc::now();
        let mut team = Team {
            id: Uuid::new_v4(),
            name: "Valid Team".to_string(),
            slug: "valid-team".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        // credits = 0 should be valid
        assert!(team.validate().is_ok());
        // credits = -1 should be invalid
        team.credits = -1;
        assert!(team.validate().is_err());
        // credits = 1 should be valid
        team.credits = 1;
        assert!(team.validate().is_ok());
    }

    #[test]
    fn test_team_validate_storage_boundary() {
        // Kill: replace < with ==, >, <= (storage < 0)
        let now = Utc::now();
        let mut team = Team {
            id: Uuid::new_v4(),
            name: "Valid Team".to_string(),
            slug: "valid-team".to_string(),
            credits: 0,
            ephemeral_storage_bytes: 0,
            settings: Json(HashMap::new()),
            created_at: now,
            updated_at: now,
        };
        // storage = 0 should be valid
        assert!(team.validate().is_ok());
        // storage = -1 should be invalid
        team.ephemeral_storage_bytes = -1;
        assert!(team.validate().is_err());
        // storage = 1 should be valid
        team.ephemeral_storage_bytes = 1;
        assert!(team.validate().is_ok());
    }

    // ========================================================================
    // Mutant-killing tests: MembershipRole::can_invite
    // ========================================================================

    #[test]
    fn test_membership_role_can_invite_true_false() {
        // Kill: replace -> bool with true and replace -> bool with false
        assert!(MembershipRole::Owner.can_invite());
        assert!(MembershipRole::Admin.can_invite());
        assert!(!MembershipRole::Member.can_invite());
        assert!(!MembershipRole::Viewer.can_invite());
    }

    // ========================================================================
    // Mutant-killing tests: InvitationRole::to_membership_role
    // ========================================================================

    #[test]
    fn test_invitation_role_to_membership_role_all_variants() {
        // Kill: replace -> MembershipRole with Default::default()
        // Default for MembershipRole is Member, so test Admin and Viewer specifically
        assert_eq!(
            InvitationRole::Admin.to_membership_role(),
            MembershipRole::Admin
        );
        assert_eq!(
            InvitationRole::Member.to_membership_role(),
            MembershipRole::Member
        );
        assert_eq!(
            InvitationRole::Viewer.to_membership_role(),
            MembershipRole::Viewer
        );
    }

    // ========================================================================
    // Mutant-killing tests: InvitationState::valid_transitions
    // ========================================================================

    #[test]
    fn test_invitation_state_valid_transitions_not_empty() {
        // Kill: replace -> Vec with vec![]
        let transitions = InvitationState::Pending.valid_transitions();
        assert!(!transitions.is_empty());
        assert!(transitions.contains(&InvitationState::Accepted));
        assert!(transitions.contains(&InvitationState::Declined));
        assert!(transitions.contains(&InvitationState::Revoked));

        // Terminal states should have empty transitions
        assert!(InvitationState::Accepted.valid_transitions().is_empty());
        assert!(InvitationState::Declined.valid_transitions().is_empty());
        assert!(InvitationState::Revoked.valid_transitions().is_empty());
        assert!(InvitationState::Expired.valid_transitions().is_empty());
    }

    // ========================================================================
    // Mutant-killing tests: Invitation
    // ========================================================================

    #[test]
    fn test_invitation_state_expired_boundary() {
        // Kill: replace < with ==, <= (expires_at < Utc::now() in state())
        let now = Utc::now();
        let mut invitation = Invitation {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            invited_by: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
            token: "token123".to_string(),
            expires_at: now - chrono::Duration::seconds(10),
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now - chrono::Duration::days(8),
        };
        // Expired invitation should be in Expired state
        assert_eq!(invitation.state(), InvitationState::Expired);

        // Future expiry should be Pending
        invitation.expires_at = now + chrono::Duration::days(7);
        assert_eq!(invitation.state(), InvitationState::Pending);
    }

    #[test]
    fn test_invitation_is_expired_method() {
        // Kill: replace is_expired with false; replace < with ==, <=
        let now = Utc::now();
        let mut invitation = Invitation {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            invited_by: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
            token: "token123".to_string(),
            expires_at: now - chrono::Duration::seconds(10),
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now - chrono::Duration::days(8),
        };
        assert!(invitation.is_expired());

        invitation.expires_at = now + chrono::Duration::days(7);
        assert!(!invitation.is_expired());
    }

    #[test]
    fn test_invitation_can_transition_true_false() {
        // Kill: replace can_transition with true and false
        let now = Utc::now();
        let invitation = Invitation {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            invited_by: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
            token: "token123".to_string(),
            expires_at: now + chrono::Duration::days(7),
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now,
        };
        // Pending invitation can accept
        assert!(invitation.can_transition(&InvitationEvent::Accept));
        // Pending invitation can decline
        assert!(invitation.can_transition(&InvitationEvent::Decline));

        // Accepted invitation cannot do anything
        let accepted = Invitation {
            accepted_at: Some(now),
            ..invitation.clone()
        };
        assert!(!accepted.can_transition(&InvitationEvent::Accept));
        assert!(!accepted.can_transition(&InvitationEvent::Decline));
        assert!(!accepted.can_transition(&InvitationEvent::Revoke));
    }

    #[test]
    fn test_invitation_validate_email_or_conditions() {
        // Kill: replace || with &&, delete ! (validate email)
        let now = Utc::now();
        // Email without '@' -> should fail
        let inv1 = Invitation {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            invited_by: Uuid::new_v4(),
            email: "noemail".to_string(),
            role: InvitationRole::Member,
            token: "token123".to_string(),
            expires_at: now + chrono::Duration::days(7),
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now,
        };
        assert!(inv1.validate().is_err());

        // Empty email -> should fail
        let inv2 = Invitation {
            email: "".to_string(),
            ..inv1.clone()
        };
        assert!(inv2.validate().is_err());

        // Valid email -> should pass
        let inv3 = Invitation {
            email: "test@example.com".to_string(),
            ..inv1.clone()
        };
        assert!(inv3.validate().is_ok());
    }

    #[test]
    fn test_invitation_validate_terminal_count_boundary() {
        // Kill: replace > with >= (terminal_count > 1)
        let now = Utc::now();
        // Exactly 1 terminal timestamp is valid
        let inv_one = Invitation {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            invited_by: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
            token: "token123".to_string(),
            expires_at: now + chrono::Duration::days(7),
            accepted_at: Some(now),
            declined_at: None,
            revoked_at: None,
            created_at: now,
        };
        assert!(inv_one.validate().is_ok());

        // 2 terminal timestamps is invalid
        let inv_two = Invitation {
            accepted_at: Some(now),
            declined_at: Some(now),
            ..inv_one.clone()
        };
        assert!(inv_two.validate().is_err());
    }

    #[test]
    fn test_invitation_validate_time_boundary() {
        // Kill: replace >= with < (created_at >= expires_at)
        let now = Utc::now();
        // created_at == expires_at should fail
        let inv_eq = Invitation {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            invited_by: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
            token: "token123".to_string(),
            expires_at: now,
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now,
        };
        assert!(inv_eq.validate().is_err());

        // created_at < expires_at should pass
        let inv_ok = Invitation {
            expires_at: now + chrono::Duration::days(7),
            ..inv_eq.clone()
        };
        assert!(inv_ok.validate().is_ok());

        // created_at > expires_at should fail
        let inv_bad = Invitation {
            expires_at: now - chrono::Duration::days(1),
            ..inv_eq.clone()
        };
        assert!(inv_bad.validate().is_err());
    }

    // ========================================================================
    // Mutant-killing tests: ApiKey
    // ========================================================================

    #[test]
    fn test_api_key_is_valid_expires_boundary() {
        // Kill: replace < with ==, >, <= (is_valid expires check)
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let now = Utc::now();

        let mut api_key = ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: owner.to_string(),
            name: "Test".to_string(),
            key_prefix: "sk_live_".to_string(),
            key_hash: "abcd:ef01".to_string(),
            scopes: Json(vec!["*".to_string()]),
            last_used_at: None,
            expires_at: Some(now - chrono::Duration::seconds(10)),
            revoked_at: None,
            created_at: now,
        };
        // Expired key should be invalid
        assert!(!api_key.is_valid());

        // Future expiry should be valid
        api_key.expires_at = Some(now + chrono::Duration::days(1));
        assert!(api_key.is_valid());

        // No expiry should be valid
        api_key.expires_at = None;
        assert!(api_key.is_valid());
    }

    #[test]
    fn test_api_key_validate_returns_err_on_invalid() {
        // Kill: replace validate with Ok(())
        let user_id = Uuid::new_v4();
        let api_key = ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: Urn::user(user_id).to_string(),
            name: "a".repeat(101),
            key_prefix: "sk_live_".to_string(),
            key_hash: "abcd:ef01".to_string(),
            scopes: Json(vec!["*".to_string()]),
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        };
        assert!(api_key.validate().is_err());
    }

    #[test]
    fn test_api_key_validate_name_len_boundary() {
        // Kill: replace > with ==, >= (name len)
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);

        let key100 = ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: owner.to_string(),
            name: "a".repeat(100),
            key_prefix: "sk_live_".to_string(),
            key_hash: "abcd:ef01".to_string(),
            scopes: Json(vec!["*".to_string()]),
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        };
        assert!(key100.validate().is_ok());

        let key101 = ApiKey {
            name: "a".repeat(101),
            ..key100.clone()
        };
        assert!(key101.validate().is_err());

        let key99 = ApiKey {
            name: "a".repeat(99),
            ..key100.clone()
        };
        assert!(key99.validate().is_ok());
    }
}
