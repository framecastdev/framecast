//! Domain entities for Framecast
//!
//! This module contains all domain entities as defined in the API specification.
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

use crate::state::{
    InvitationEvent, InvitationGuardContext, InvitationState as StateMachineInvitationState,
    InvitationStateMachine, JobEvent, JobState, JobStateMachine, ProjectEvent, ProjectState,
    ProjectStateMachine, StateError, WebhookDeliveryEvent, WebhookDeliveryGuardContext,
    WebhookDeliveryState, WebhookDeliveryStateMachine,
};

/// Maximum number of team memberships a single user can hold (INV-T8)
pub const MAX_TEAM_MEMBERSHIPS: i64 = 50;

/// User tier levels
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
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
        // Validate email format
        if !email.contains('@') || email.len() > 255 {
            return Err(Error::Validation(
                "Invalid email format or length".to_string(),
            ));
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

        // Email validation
        if !self.email.contains('@') || self.email.len() > 255 {
            return Err(Error::Validation(
                "Invalid email format or length".to_string(),
            ));
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

        Ok(())
    }

    /// Generate slug from name with random suffix if needed
    fn generate_slug(name: &str) -> Result<String> {
        let base = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "membership_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MembershipRole {
    Owner,
    Admin,
    #[default]
    Member,
    Viewer,
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
        self.is_owner()
    }
}

/// Role for invitation (excludes Owner since owners cannot be invited)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, sqlx::Type)]
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

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Basic reference validation is handled by database constraints
        Ok(())
    }
}

/// Invitation states (derived from timestamps)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvitationState {
    Pending,
    Accepted,
    Declined,
    Revoked,
    Expired,
}

impl InvitationState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> StateMachineInvitationState {
        match self {
            InvitationState::Pending => StateMachineInvitationState::Pending,
            InvitationState::Accepted => StateMachineInvitationState::Accepted,
            InvitationState::Declined => StateMachineInvitationState::Declined,
            InvitationState::Revoked => StateMachineInvitationState::Revoked,
            InvitationState::Expired => StateMachineInvitationState::Expired,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: StateMachineInvitationState) -> Self {
        match state {
            StateMachineInvitationState::Pending => InvitationState::Pending,
            StateMachineInvitationState::Accepted => InvitationState::Accepted,
            StateMachineInvitationState::Declined => InvitationState::Declined,
            StateMachineInvitationState::Revoked => InvitationState::Revoked,
            StateMachineInvitationState::Expired => InvitationState::Expired,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<InvitationState> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| InvitationState::from_state(*s))
            .collect()
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
        if !email.contains('@') || email.is_empty() {
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
    fn apply_transition(&self, event: InvitationEvent) -> Result<StateMachineInvitationState> {
        let current_state = self.state().to_state();
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
        InvitationStateMachine::can_transition(self.state().to_state(), event, Some(&context))
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Email validation
        if !self.email.contains('@') || self.email.is_empty() {
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
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

impl ApiKey {
    /// Create a new API key with validation
    pub fn new(
        user_id: Uuid,
        owner: Urn,
        name: Option<String>,
        scopes: Option<Vec<String>>,
    ) -> Result<Self> {
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

        Ok(ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: owner.to_string(),
            name,
            key_prefix,
            key_hash,
            scopes: Json(scopes),
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        })
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
        // Parse stored hash: salt:hash
        let parts: Vec<&str> = self.key_hash.split(':').collect();
        if parts.len() != 2 {
            return false;
        }

        let stored_salt = match hex::decode(parts[0]) {
            Ok(salt) => salt,
            Err(_) => return false,
        };

        let stored_hash = match hex::decode(parts[1]) {
            Ok(hash) => hash,
            Err(_) => return false,
        };

        // Compute hash of candidate key with stored salt
        let mut hasher = Sha256::new();
        hasher.update(candidate_key.as_bytes());
        hasher.update(&stored_salt);
        let candidate_hash = hasher.finalize();

        // Constant-time comparison to prevent timing attacks
        if stored_hash.len() != candidate_hash.len() {
            return false;
        }

        let mut result = 0u8;
        for (a, b) in stored_hash.iter().zip(candidate_hash.iter()) {
            result |= a ^ b;
        }
        result == 0
    }
}

/// Project status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "project_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    #[default]
    Draft,
    Rendering,
    Completed,
    Archived,
}

impl ProjectStatus {
    /// Check if this is a terminal state (Project has no terminal states)
    #[mutants::skip] // Delegates to ProjectState::is_terminal() which always returns false
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> ProjectState {
        match self {
            ProjectStatus::Draft => ProjectState::Draft,
            ProjectStatus::Rendering => ProjectState::Rendering,
            ProjectStatus::Completed => ProjectState::Completed,
            ProjectStatus::Archived => ProjectState::Archived,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: ProjectState) -> Self {
        match state {
            ProjectState::Draft => ProjectStatus::Draft,
            ProjectState::Rendering => ProjectStatus::Rendering,
            ProjectState::Completed => ProjectStatus::Completed,
            ProjectState::Archived => ProjectStatus::Archived,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<ProjectStatus> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| ProjectStatus::from_state(*s))
            .collect()
    }
}

/// Project entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub team_id: Uuid,
    pub created_by: Uuid,
    pub name: String,
    pub status: ProjectStatus,
    pub spec: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    /// Create a new project with validation
    pub fn new(
        team_id: Uuid,
        created_by: Uuid,
        name: String,
        spec: serde_json::Value,
    ) -> Result<Self> {
        if name.len() > 200 {
            return Err(Error::Validation(
                "Project name must be ≤200 characters".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Project {
            id: Uuid::new_v4(),
            team_id,
            created_by,
            name,
            status: ProjectStatus::default(),
            spec: Json(spec),
            created_at: now,
            updated_at: now,
        })
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        if self.name.len() > 200 {
            return Err(Error::Validation(
                "Project name must be ≤200 characters".to_string(),
            ));
        }
        Ok(())
    }

    /// Start rendering the project
    pub fn start_render(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::Render)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark project as completed (called when job completes)
    pub fn on_job_completed(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::JobCompleted)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark project as draft (called when job fails)
    pub fn on_job_failed(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::JobFailed)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark project as draft (called when job is canceled)
    pub fn on_job_canceled(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::JobCanceled)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Archive the project
    pub fn archive(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::Archive)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Unarchive the project
    pub fn unarchive(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::Unarchive)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: ProjectEvent) -> Result<ProjectState> {
        let current_state = self.status.to_state();
        ProjectStateMachine::transition(current_state, event).map_err(|e| match e {
            StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                "Invalid project transition: cannot apply '{}' event from '{}' state",
                event, from
            )),
            StateError::TerminalState(state) => Error::Validation(format!(
                "Project is in terminal state '{}' and cannot transition",
                state
            )),
            StateError::GuardFailed(msg) => Error::Validation(msg),
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &ProjectEvent) -> bool {
        ProjectStateMachine::can_transition(self.status.to_state(), event)
    }
}

/// Job status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "job_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    #[default]
    Queued,
    Processing,
    Completed,
    Failed,
    Canceled,
}

impl JobStatus {
    /// Check if status is terminal (job has finished)
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> JobState {
        match self {
            JobStatus::Queued => JobState::Queued,
            JobStatus::Processing => JobState::Processing,
            JobStatus::Completed => JobState::Completed,
            JobStatus::Failed => JobState::Failed,
            JobStatus::Canceled => JobState::Canceled,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: JobState) -> Self {
        match state {
            JobState::Queued => JobStatus::Queued,
            JobState::Processing => JobStatus::Processing,
            JobState::Completed => JobStatus::Completed,
            JobState::Failed => JobStatus::Failed,
            JobState::Canceled => JobStatus::Canceled,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<JobStatus> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| JobStatus::from_state(*s))
            .collect()
    }
}

/// Job failure type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "job_failure_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum JobFailureType {
    System,
    Validation,
    Timeout,
    Canceled,
}

/// Job entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub owner: String, // URN as string
    pub triggered_by: Uuid,
    pub project_id: Option<Uuid>,
    pub status: JobStatus,
    pub spec_snapshot: Json<serde_json::Value>,
    pub options: Json<serde_json::Value>,
    pub progress: Json<serde_json::Value>,
    pub output: Option<Json<serde_json::Value>>,
    pub output_size_bytes: Option<i64>,
    pub error: Option<Json<serde_json::Value>>,
    pub credits_charged: i32,
    pub failure_type: Option<JobFailureType>,
    pub credits_refunded: i32,
    pub idempotency_key: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    /// Create a new job with validation
    pub fn new(
        owner: Urn,
        triggered_by: Uuid,
        project_id: Option<Uuid>,
        spec_snapshot: serde_json::Value,
        credits_charged: i32,
        idempotency_key: Option<String>,
    ) -> Result<Self> {
        // Validate credits
        if credits_charged < 0 {
            return Err(Error::Validation(
                "Credits charged cannot be negative".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Job {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            triggered_by,
            project_id,
            status: JobStatus::default(),
            spec_snapshot: Json(spec_snapshot),
            options: Json(serde_json::Value::Object(serde_json::Map::new())),
            progress: Json(serde_json::Value::Object(serde_json::Map::new())),
            output: None,
            output_size_bytes: None,
            error: None,
            credits_charged,
            failure_type: None,
            credits_refunded: 0,
            idempotency_key,
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Check if job is ephemeral (not tied to project)
    pub fn is_ephemeral(&self) -> bool {
        self.project_id.is_none()
    }

    /// Check if job is terminal
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Get net credits (charged - refunded)
    pub fn net_credits(&self) -> i32 {
        self.credits_charged - self.credits_refunded
    }

    /// Start job processing
    pub fn start(&mut self) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::WorkerPicksUp)?;
        self.status = JobStatus::from_state(new_state);
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Complete job successfully
    pub fn complete(
        &mut self,
        output: serde_json::Value,
        output_size_bytes: Option<i64>,
    ) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::Success)?;
        self.status = JobStatus::from_state(new_state);
        self.output = Some(Json(output));
        self.output_size_bytes = output_size_bytes;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Fail job
    pub fn fail(&mut self, error: serde_json::Value, failure_type: JobFailureType) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::Failure)?;
        self.status = JobStatus::from_state(new_state);
        self.error = Some(Json(error));
        self.failure_type = Some(failure_type.clone());

        // Apply refund based on failure type and progress
        self.apply_refund(failure_type);

        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Cancel job
    pub fn cancel(&mut self) -> Result<()> {
        let new_state = self.apply_transition(JobEvent::Cancel)?;
        self.status = JobStatus::from_state(new_state);
        self.failure_type = Some(JobFailureType::Canceled);

        // Apply refund with 10% cancellation fee
        self.apply_refund(JobFailureType::Canceled);

        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: JobEvent) -> Result<JobState> {
        let current_state = self.status.to_state();
        JobStateMachine::transition(current_state, event).map_err(|e| match e {
            StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                "Invalid job transition: cannot apply '{}' event from '{}' state",
                event, from
            )),
            StateError::TerminalState(state) => Error::Validation(format!(
                "Job is in terminal state '{}' and cannot transition",
                state
            )),
            StateError::GuardFailed(msg) => Error::Validation(msg),
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &JobEvent) -> bool {
        JobStateMachine::can_transition(self.status.to_state(), event)
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Calculate refund amount based on failure type and progress
    pub fn calculate_refund(&self, failure_type: JobFailureType) -> i32 {
        let progress_percent_raw = self.get_progress_percent();

        // Convert to integer with 2 decimal precision (10000 = 100.00%)
        let progress_int = (progress_percent_raw * 100.0).round() as i32;
        let progress_int = progress_int.clamp(0, 10000); // 0.00% to 100.00%

        match failure_type {
            // Full refund for system errors and timeouts
            JobFailureType::System | JobFailureType::Timeout => self.credits_charged,

            // Partial refund based on remaining work for validation errors
            JobFailureType::Validation => {
                let remaining_work_int = 10000 - progress_int; // Remaining work as integer
                                                               // FLOOR operation: integer division automatically floors for positive numbers
                                                               // Use i64 for intermediate calculation to prevent overflow
                let result = (self.credits_charged as i64 * remaining_work_int as i64) / 10000;
                result as i32 // Safe because result will be <= self.credits_charged (which fits in i32)
            }

            // Partial refund with 10% cancellation fee
            JobFailureType::Canceled => {
                let remaining_work_int = 10000 - progress_int;

                // Calculate: credits * remaining_work * 0.9 using i64 to prevent overflow
                // = (credits * remaining_work * 9000) / (10000 * 10000)
                let refund_before_cap =
                    (self.credits_charged as i64 * remaining_work_int as i64 * 9000) / 100_000_000; // 10000 * 10000

                // Enforce minimum 10% charge (maximum 90% refund) - SPEC REQUIREMENT
                let max_refund = (self.credits_charged as i64 * 9000) / 10000; // 90% of charged amount

                std::cmp::min(refund_before_cap as i32, max_refund as i32)
            }
        }
    }

    /// Get progress percentage from progress field
    pub fn get_progress_percent(&self) -> f64 {
        let raw_progress = self
            .progress
            .0
            .get("percent")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Round to 2 decimal places to avoid precision issues
        let rounded = (raw_progress * 100.0).round() / 100.0;
        rounded.clamp(0.0, 100.0)
    }

    /// Apply refund to the job based on failure type
    pub fn apply_refund(&mut self, failure_type: JobFailureType) {
        self.credits_refunded = self.calculate_refund(failure_type);
    }

    /// Update progress percentage
    pub fn update_progress(&mut self, percent: f64) -> Result<()> {
        if !(0.0..=100.0).contains(&percent) {
            return Err(Error::Validation(
                "Progress must be between 0 and 100".to_string(),
            ));
        }

        if let Some(progress_obj) = self.progress.0.as_object_mut() {
            progress_obj.insert(
                "percent".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(percent)
                        .ok_or_else(|| Error::Validation("Invalid progress value".to_string()))?,
                ),
            );
        } else {
            // Create new progress object
            let mut progress_map = serde_json::Map::new();
            progress_map.insert(
                "percent".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(percent)
                        .ok_or_else(|| Error::Validation("Invalid progress value".to_string()))?,
                ),
            );
            self.progress = Json(serde_json::Value::Object(progress_map));
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // INV-J8: Cannot refund more than charged
        if self.credits_refunded > self.credits_charged {
            return Err(Error::Validation(
                "Cannot refund more than charged".to_string(),
            ));
        }

        // INV-J9: Credits values cannot be negative
        if self.credits_refunded < 0 || self.credits_charged < 0 {
            return Err(Error::Validation(
                "Credits values cannot be negative".to_string(),
            ));
        }

        // INV-J2: Terminal jobs have completion timestamp
        if self.is_terminal() && self.completed_at.is_none() {
            return Err(Error::Validation(
                "Terminal jobs must have completion timestamp".to_string(),
            ));
        }

        // INV-J3: Processing jobs have start timestamp
        if self.status == JobStatus::Processing && self.started_at.is_none() {
            return Err(Error::Validation(
                "Processing jobs must have start timestamp".to_string(),
            ));
        }

        // INV-J4: Completed jobs must have output
        if self.status == JobStatus::Completed && self.output.is_none() {
            return Err(Error::Validation(
                "Completed jobs must have output".to_string(),
            ));
        }

        // INV-J5: Failed jobs must have error
        if self.status == JobStatus::Failed && self.error.is_none() {
            return Err(Error::Validation("Failed jobs must have error".to_string()));
        }

        // INV-J6 & J7: Failure type consistency
        match (&self.status, &self.failure_type) {
            (JobStatus::Failed | JobStatus::Canceled, None) => {
                return Err(Error::Validation(
                    "Failed/canceled jobs must have failure type".to_string(),
                ));
            }
            (JobStatus::Completed, Some(_)) => {
                return Err(Error::Validation(
                    "Completed jobs must not have failure type".to_string(),
                ));
            }
            _ => {}
        }

        // INV-J11: Project jobs must be team-owned
        if self.project_id.is_some() {
            let urn = self.owner_urn()?;
            if !urn.is_team() {
                return Err(Error::Validation(
                    "Project-based jobs must be team-owned".to_string(),
                ));
            }
        }

        // SPEC: Cancellation must charge at least 10% (maximum 90% refund)
        if let Some(JobFailureType::Canceled) = self.failure_type {
            let min_charge = (self.credits_charged * 10) / 100; // 10% minimum
            let actual_charge = self.credits_charged - self.credits_refunded;
            if actual_charge < min_charge {
                return Err(Error::Validation(
                    "Cancellation must charge at least 10%".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Asset file status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "asset_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AssetStatus {
    #[default]
    Pending,
    Ready,
    Failed,
}

/// Asset file entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AssetFile {
    pub id: Uuid,
    pub owner: String, // URN as string
    pub uploaded_by: Uuid,
    pub project_id: Option<Uuid>,
    pub filename: String,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: AssetStatus,
    pub metadata: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AssetFile {
    /// Allowed content types per spec
    pub const ALLOWED_CONTENT_TYPES: &'static [&'static str] = &[
        "image/jpeg",
        "image/png",
        "image/webp",
        "audio/mpeg",
        "audio/wav",
        "audio/ogg",
        "video/mp4",
    ];

    /// Maximum file size (50MB)
    pub const MAX_SIZE_BYTES: i64 = 50 * 1024 * 1024;

    /// Create a new asset file with validation
    pub fn new(
        owner: Urn,
        uploaded_by: Uuid,
        project_id: Option<Uuid>,
        filename: String,
        s3_key: String,
        content_type: String,
        size_bytes: i64,
    ) -> Result<Self> {
        // Validate filename
        if filename.is_empty() || filename.len() > 255 {
            return Err(Error::Validation(
                "Filename must be 1-255 characters".to_string(),
            ));
        }

        // Validate content type
        if !Self::ALLOWED_CONTENT_TYPES.contains(&content_type.as_str()) {
            return Err(Error::Validation(format!(
                "Content type '{}' not allowed",
                content_type
            )));
        }

        // Validate size
        if size_bytes <= 0 {
            return Err(Error::Validation("File size must be positive".to_string()));
        }

        if size_bytes > Self::MAX_SIZE_BYTES {
            return Err(Error::Validation(format!(
                "File size exceeds maximum of {} bytes",
                Self::MAX_SIZE_BYTES
            )));
        }

        let now = Utc::now();
        Ok(AssetFile {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            uploaded_by,
            project_id,
            filename,
            s3_key,
            content_type,
            size_bytes,
            status: AssetStatus::default(),
            metadata: Json(serde_json::Value::Object(serde_json::Map::new())),
            created_at: now,
            updated_at: now,
        })
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Filename validation
        if self.filename.is_empty() || self.filename.len() > 255 {
            return Err(Error::Validation(
                "Filename must be 1-255 characters".to_string(),
            ));
        }

        // Content type validation
        if !Self::ALLOWED_CONTENT_TYPES.contains(&self.content_type.as_str()) {
            return Err(Error::Validation(format!(
                "Content type '{}' not allowed",
                self.content_type
            )));
        }

        // Size validation
        if self.size_bytes <= 0 {
            return Err(Error::Validation("File size must be positive".to_string()));
        }

        if self.size_bytes > Self::MAX_SIZE_BYTES {
            return Err(Error::Validation(format!(
                "File size exceeds maximum of {} bytes",
                Self::MAX_SIZE_BYTES
            )));
        }

        Ok(())
    }
}

/// Webhook entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Webhook {
    pub id: Uuid,
    pub team_id: Uuid,
    pub created_by: Uuid,
    pub url: String,
    pub events: Json<Vec<String>>,
    pub secret: String,
    pub is_active: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Webhook {
    /// Valid webhook events per spec
    pub const VALID_EVENTS: &'static [&'static str] = &[
        "job.queued",
        "job.started",
        "job.progress",
        "job.completed",
        "job.failed",
        "job.canceled",
    ];

    /// Create a new webhook with validation
    pub fn new(team_id: Uuid, created_by: Uuid, url: String, events: Vec<String>) -> Result<Self> {
        // Validate URL
        if !url.starts_with("https://") {
            return Err(Error::Validation("Webhook URL must be HTTPS".to_string()));
        }

        if url.len() > 2048 {
            return Err(Error::Validation(
                "URL must be ≤2048 characters".to_string(),
            ));
        }

        // Validate events
        if events.is_empty() {
            return Err(Error::Validation(
                "Must subscribe to at least one event".to_string(),
            ));
        }

        for event in &events {
            if !Self::VALID_EVENTS.contains(&event.as_str()) {
                return Err(Error::Validation(format!("Invalid event: {}", event)));
            }
        }

        // Generate secret for HMAC signing
        let secret = uuid::Uuid::new_v4().to_string().replace('-', "");

        let now = Utc::now();
        Ok(Webhook {
            id: Uuid::new_v4(),
            team_id,
            created_by,
            url,
            events: Json(events),
            secret,
            is_active: true,
            last_triggered_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // URL validation
        if !self.url.starts_with("https://") {
            return Err(Error::Validation("Webhook URL must be HTTPS".to_string()));
        }

        if self.url.len() > 2048 {
            return Err(Error::Validation(
                "URL must be ≤2048 characters".to_string(),
            ));
        }

        // Events validation
        if self.events.is_empty() {
            return Err(Error::Validation(
                "Must subscribe to at least one event".to_string(),
            ));
        }

        for event in self.events.iter() {
            if !Self::VALID_EVENTS.contains(&event.as_str()) {
                return Err(Error::Validation(format!("Invalid event: {}", event)));
            }
        }

        Ok(())
    }
}

/// Webhook delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "webhook_delivery_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WebhookDeliveryStatus {
    #[default]
    Pending,
    Attempting,
    Delivered,
    Retrying,
    Failed,
}

impl WebhookDeliveryStatus {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> WebhookDeliveryState {
        match self {
            WebhookDeliveryStatus::Pending => WebhookDeliveryState::Pending,
            WebhookDeliveryStatus::Attempting => WebhookDeliveryState::Attempting,
            WebhookDeliveryStatus::Delivered => WebhookDeliveryState::Delivered,
            WebhookDeliveryStatus::Retrying => WebhookDeliveryState::Retrying,
            WebhookDeliveryStatus::Failed => WebhookDeliveryState::Failed,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: WebhookDeliveryState) -> Self {
        match state {
            WebhookDeliveryState::Pending => WebhookDeliveryStatus::Pending,
            WebhookDeliveryState::Attempting => WebhookDeliveryStatus::Attempting,
            WebhookDeliveryState::Delivered => WebhookDeliveryStatus::Delivered,
            WebhookDeliveryState::Retrying => WebhookDeliveryStatus::Retrying,
            WebhookDeliveryState::Failed => WebhookDeliveryStatus::Failed,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<WebhookDeliveryStatus> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| WebhookDeliveryStatus::from_state(*s))
            .collect()
    }
}

/// Webhook delivery entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub job_id: Option<Uuid>,
    pub event_type: String,
    pub status: WebhookDeliveryStatus,
    pub payload: Json<serde_json::Value>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl WebhookDelivery {
    /// Create a new webhook delivery
    pub fn new(
        webhook_id: Uuid,
        job_id: Option<Uuid>,
        event_type: String,
        payload: serde_json::Value,
    ) -> Self {
        WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id,
            job_id,
            event_type,
            status: WebhookDeliveryStatus::default(),
            payload: Json(payload),
            response_status: None,
            response_body: None,
            attempts: 0,
            max_attempts: 5,
            next_retry_at: None,
            delivered_at: None,
            created_at: Utc::now(),
        }
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Attempts validation
        if self.attempts > self.max_attempts {
            return Err(Error::Validation(
                "Attempts cannot exceed maximum".to_string(),
            ));
        }

        // Delivery validation
        if self.status == WebhookDeliveryStatus::Delivered && self.delivered_at.is_none() {
            return Err(Error::Validation(
                "Delivered webhooks must have delivery timestamp".to_string(),
            ));
        }

        Ok(())
    }

    /// Start an attempt to deliver the webhook
    pub fn start_attempt(&mut self) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::Attempt)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.attempts += 1;
        Ok(())
    }

    /// Mark delivery as successful (2xx response)
    pub fn mark_delivered(
        &mut self,
        response_status: i32,
        response_body: Option<String>,
    ) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::Success)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.response_status = Some(response_status);
        self.response_body = response_body;
        self.delivered_at = Some(Utc::now());
        self.next_retry_at = None;
        Ok(())
    }

    /// Mark for retry (5xx or timeout)
    pub fn mark_for_retry(
        &mut self,
        response_status: Option<i32>,
        response_body: Option<String>,
        next_retry_at: DateTime<Utc>,
    ) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::Retry)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.response_status = response_status;
        self.response_body = response_body;
        self.next_retry_at = Some(next_retry_at);
        Ok(())
    }

    /// Mark as permanently failed (4xx response)
    pub fn mark_failed_permanent(
        &mut self,
        response_status: i32,
        response_body: Option<String>,
    ) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::PermanentFailure)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.response_status = Some(response_status);
        self.response_body = response_body;
        self.next_retry_at = None;
        Ok(())
    }

    /// Mark as failed due to max attempts exceeded
    pub fn mark_failed_max_attempts(&mut self) -> Result<()> {
        let new_state = self.apply_transition(WebhookDeliveryEvent::MaxAttemptsExceeded)?;
        self.status = WebhookDeliveryStatus::from_state(new_state);
        self.next_retry_at = None;
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: WebhookDeliveryEvent) -> Result<WebhookDeliveryState> {
        let current_state = self.status.to_state();
        let context = WebhookDeliveryGuardContext {
            attempt_count: self.attempts as u32,
            max_attempts: self.max_attempts as u32,
        };
        WebhookDeliveryStateMachine::transition(current_state, event, Some(&context)).map_err(|e| {
            match e {
                StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                    "Invalid webhook delivery transition: cannot apply '{}' event from '{}' state",
                    event, from
                )),
                StateError::TerminalState(state) => Error::Validation(format!(
                    "Webhook delivery is in terminal state '{}' and cannot transition",
                    state
                )),
                StateError::GuardFailed(msg) => Error::Validation(msg),
            }
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &WebhookDeliveryEvent) -> bool {
        let context = WebhookDeliveryGuardContext {
            attempt_count: self.attempts as u32,
            max_attempts: self.max_attempts as u32,
        };
        WebhookDeliveryStateMachine::can_transition(self.status.to_state(), event, Some(&context))
    }
}

/// Usage entity for billing metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Usage {
    pub id: Uuid,
    pub owner: String,  // URN as string
    pub period: String, // Format: YYYY-MM
    pub renders_count: i32,
    pub render_seconds: i32,
    pub credits_used: i32,
    pub credits_refunded: i32,
    pub api_calls: i32,
    pub updated_at: DateTime<Utc>,
}

impl Usage {
    /// Create new usage record
    pub fn new(owner: Urn, period: String) -> Result<Self> {
        // Validate period format (YYYY-MM)
        if period.len() != 7 {
            return Err(Error::Validation(
                "Period must be YYYY-MM format".to_string(),
            ));
        }

        let regex = regex::Regex::new(r"^\d{4}-(0[1-9]|1[0-2])$").unwrap();
        if !regex.is_match(&period) {
            return Err(Error::Validation(
                "Period must be YYYY-MM format".to_string(),
            ));
        }

        Ok(Usage {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            period,
            renders_count: 0,
            render_seconds: 0,
            credits_used: 0,
            credits_refunded: 0,
            api_calls: 0,
            updated_at: Utc::now(),
        })
    }

    /// Get net credits (used - refunded)
    pub fn net_credits(&self) -> i32 {
        self.credits_used - self.credits_refunded
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Period format validation - check length first, then regex
        if self.period.len() != 7 {
            return Err(Error::Validation(
                "Period format must be YYYY-MM".to_string(),
            ));
        }

        let regex = regex::Regex::new(r"^\d{4}-(0[1-9]|1[0-2])$").unwrap();
        if !regex.is_match(&self.period) {
            return Err(Error::Validation(
                "Period format must be YYYY-MM".to_string(),
            ));
        }

        // Counts cannot be negative
        if self.renders_count < 0 || self.credits_used < 0 || self.api_calls < 0 {
            return Err(Error::Validation(
                "Usage counts cannot be negative".to_string(),
            ));
        }

        Ok(())
    }
}

/// System asset categories
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "system_asset_category", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SystemAssetCategory {
    Sfx,
    Ambient,
    Music,
    Transition,
}

/// System asset entity for pre-loaded assets
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SystemAsset {
    pub id: String, // Format: asset_{category}_{name}
    pub category: SystemAssetCategory,
    pub name: String,
    pub description: String,
    pub duration_seconds: Option<rust_decimal::Decimal>,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub tags: Json<Vec<String>>,
    pub created_at: DateTime<Utc>,
}

impl SystemAsset {
    /// Create new system asset with validation
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        category: SystemAssetCategory,
        name: String,
        description: String,
        s3_key: String,
        content_type: String,
        size_bytes: i64,
        duration_seconds: Option<rust_decimal::Decimal>,
        tags: Vec<String>,
    ) -> Result<Self> {
        // Generate and validate ID format
        let category_str = match category {
            SystemAssetCategory::Sfx => "sfx",
            SystemAssetCategory::Ambient => "ambient",
            SystemAssetCategory::Music => "music",
            SystemAssetCategory::Transition => "transition",
        };

        let id = format!("asset_{}_{}", category_str, name);

        // Validate ID format
        let id_regex =
            regex::Regex::new(r"^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$").unwrap();
        if !id_regex.is_match(&id) {
            return Err(Error::Validation(
                "Invalid system asset ID format".to_string(),
            ));
        }

        // Validate description length
        if description.len() > 500 {
            return Err(Error::Validation(
                "Description must be ≤500 characters".to_string(),
            ));
        }

        Ok(SystemAsset {
            id,
            category,
            name,
            description,
            duration_seconds,
            s3_key,
            content_type,
            size_bytes,
            tags: Json(tags),
            created_at: Utc::now(),
        })
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // ID format validation
        let id_regex =
            regex::Regex::new(r"^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$").unwrap();
        if !id_regex.is_match(&self.id) {
            return Err(Error::Validation(
                "Invalid system asset ID format".to_string(),
            ));
        }

        // Description length validation
        if self.description.len() > 500 {
            return Err(Error::Validation(
                "Description must be ≤500 characters".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!(!MembershipRole::Admin.can_modify_team());
    }

    #[test]
    fn test_invitation_creation() {
        let team_id = Uuid::new_v4();
        let invited_by = Uuid::new_v4();
        let email = "invitee@example.com".to_string();
        let role = InvitationRole::Member;

        let invitation = Invitation::new(team_id, invited_by, email.clone(), role.clone()).unwrap();

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

        let api_key = ApiKey::new(user_id, owner.clone(), name.clone(), None).unwrap();

        assert_eq!(api_key.user_id, user_id);
        assert_eq!(api_key.owner_urn().unwrap(), owner);
        assert_eq!(api_key.name, "Test Key");
        assert!(api_key.key_prefix.starts_with("sk_live_"));
        assert!(!api_key.key_hash.is_empty());
        assert!(api_key.is_valid());
    }

    #[test]
    fn test_api_key_validation() {
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);

        // Test name too long
        let result = ApiKey::new(user_id, owner.clone(), Some("a".repeat(101)), None);
        assert!(result.is_err());

        // Test valid key
        let api_key = ApiKey::new(user_id, owner, None, None).unwrap();
        assert!(api_key.validate().is_ok());
    }

    #[test]
    fn test_api_key_revocation() {
        let user_id = Uuid::new_v4();
        let mut api_key = ApiKey::new(user_id, Urn::user(user_id), None, None).unwrap();

        assert!(api_key.is_valid());

        api_key.revoke();
        assert!(!api_key.is_valid());
        assert!(api_key.revoked_at.is_some());
    }

    #[test]
    fn test_api_key_secure_hashing_and_verification() {
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);

        // Create API key with secure hashing
        let api_key = ApiKey::new(user_id, owner.clone(), None, None).unwrap();

        // The hash should be in salt:hash format with hex encoding
        assert!(api_key.key_hash.contains(':'));
        let parts: Vec<&str> = api_key.key_hash.split(':').collect();
        assert_eq!(parts.len(), 2);

        // Both salt and hash should be valid hex
        assert!(hex::decode(parts[0]).is_ok());
        assert!(hex::decode(parts[1]).is_ok());

        // The hash should be 64 characters (SHA-256 = 32 bytes = 64 hex chars)
        assert_eq!(parts[1].len(), 64);

        // NOTE: Since we can't access the original key from the creation,
        // we'll test the verification logic with a known key
        let test_key = "sk_live_test123456789";
        let salt: [u8; 32] = [42; 32]; // Fixed salt for testing
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
    fn test_project_creation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let name = "Test Project".to_string();
        let spec = json!({"type": "storyboard", "scenes": []});

        let project = Project::new(team_id, created_by, name.clone(), spec.clone()).unwrap();

        assert_eq!(project.team_id, team_id);
        assert_eq!(project.created_by, created_by);
        assert_eq!(project.name, name);
        assert_eq!(project.status, ProjectStatus::Draft);
        assert_eq!(project.spec.0, spec);
    }

    #[test]
    fn test_project_name_validation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let spec = json!({});

        // Test name too long
        let result = Project::new(team_id, created_by, "a".repeat(201), spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_job_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();
        let spec = json!({"type": "render", "duration": 30});
        let credits_charged = 100;

        let job = Job::new(
            owner.clone(),
            triggered_by,
            None,
            spec.clone(),
            credits_charged,
            None,
        )
        .unwrap();

        assert_eq!(job.owner_urn().unwrap(), owner);
        assert_eq!(job.triggered_by, triggered_by);
        assert!(job.is_ephemeral());
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.credits_charged, credits_charged);
        assert_eq!(job.credits_refunded, 0);
        assert!(!job.is_terminal());
    }

    #[test]
    fn test_job_state_transitions() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();
        let mut job = Job::new(owner, triggered_by, None, json!({}), 100, None).unwrap();

        // Start job
        job.start().unwrap();
        assert_eq!(job.status, JobStatus::Processing);
        assert!(job.started_at.is_some());

        // Complete job
        let output = json!({"url": "https://example.com/video.mp4"});
        job.complete(output.clone(), Some(1024)).unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.output.is_some());
        assert_eq!(job.output_size_bytes, Some(1024));
        assert!(job.is_terminal());
    }

    #[test]
    fn test_job_failure() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();
        let mut job = Job::new(owner, triggered_by, None, json!({}), 100, None).unwrap();

        job.start().unwrap();

        let error = json!({"message": "Rendering failed", "code": "RENDER_ERROR"});
        job.fail(error.clone(), JobFailureType::System).unwrap();

        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.is_some());
        assert_eq!(job.failure_type, Some(JobFailureType::System));
        assert!(job.is_terminal());
    }

    #[test]
    fn test_job_invariants() {
        let owner = Urn::user(Uuid::new_v4());
        let triggered_by = Uuid::new_v4();

        // Test negative credits
        let result = Job::new(owner.clone(), triggered_by, None, json!({}), -1, None);
        assert!(result.is_err());

        let mut job = Job::new(owner, triggered_by, None, json!({}), 100, None).unwrap();

        // Valid job
        assert!(job.validate().is_ok());

        // Invalid: refund more than charged
        job.credits_refunded = 150;
        assert!(job.validate().is_err());
    }

    #[test]
    fn test_job_project_team_constraint() {
        let team_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let team_owner = Urn::team(team_id);
        let user_owner = Urn::user(user_id);

        // Project job must be team-owned
        let project_job = Job::new(
            team_owner,
            user_id,
            Some(Uuid::new_v4()), // project_id
            json!({}),
            100,
            None,
        )
        .unwrap();
        assert!(project_job.validate().is_ok());

        // Project job cannot be user-owned
        let invalid_job = Job::new(
            user_owner,
            user_id,
            Some(Uuid::new_v4()), // project_id
            json!({}),
            100,
            None,
        )
        .unwrap();
        assert!(invalid_job.validate().is_err());
    }

    #[test]
    fn test_asset_file_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let uploaded_by = Uuid::new_v4();
        let filename = "test.jpg".to_string();
        let s3_key = "uploads/test.jpg".to_string();
        let content_type = "image/jpeg".to_string();
        let size_bytes = 1024;

        let asset = AssetFile::new(
            owner.clone(),
            uploaded_by,
            None,
            filename.clone(),
            s3_key.clone(),
            content_type.clone(),
            size_bytes,
        )
        .unwrap();

        assert_eq!(asset.owner_urn().unwrap(), owner);
        assert_eq!(asset.uploaded_by, uploaded_by);
        assert_eq!(asset.filename, filename);
        assert_eq!(asset.s3_key, s3_key);
        assert_eq!(asset.content_type, content_type);
        assert_eq!(asset.size_bytes, size_bytes);
        assert_eq!(asset.status, AssetStatus::Pending);
    }

    #[test]
    fn test_asset_file_validation() {
        let owner = Urn::user(Uuid::new_v4());
        let uploaded_by = Uuid::new_v4();

        // Test invalid content type
        let result = AssetFile::new(
            owner.clone(),
            uploaded_by,
            None,
            "test.txt".to_string(),
            "uploads/test.txt".to_string(),
            "text/plain".to_string(), // Not allowed
            1024,
        );
        assert!(result.is_err());

        // Test file too large
        let result = AssetFile::new(
            owner.clone(),
            uploaded_by,
            None,
            "large.jpg".to_string(),
            "uploads/large.jpg".to_string(),
            "image/jpeg".to_string(),
            AssetFile::MAX_SIZE_BYTES + 1,
        );
        assert!(result.is_err());

        // Test valid file
        let asset = AssetFile::new(
            owner,
            uploaded_by,
            None,
            "test.jpg".to_string(),
            "uploads/test.jpg".to_string(),
            "image/jpeg".to_string(),
            1024,
        )
        .unwrap();
        assert!(asset.validate().is_ok());
    }

    #[test]
    fn test_webhook_creation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let url = "https://example.com/webhook".to_string();
        let events = vec!["job.completed".to_string(), "job.failed".to_string()];

        let webhook = Webhook::new(team_id, created_by, url.clone(), events.clone()).unwrap();

        assert_eq!(webhook.team_id, team_id);
        assert_eq!(webhook.created_by, created_by);
        assert_eq!(webhook.url, url);
        assert_eq!(webhook.events.0, events);
        assert!(!webhook.secret.is_empty());
        assert!(webhook.is_active);
    }

    #[test]
    fn test_webhook_validation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();

        // Test non-HTTPS URL
        let result = Webhook::new(
            team_id,
            created_by,
            "http://example.com/webhook".to_string(),
            vec!["job.completed".to_string()],
        );
        assert!(result.is_err());

        // Test invalid event
        let result = Webhook::new(
            team_id,
            created_by,
            "https://example.com/webhook".to_string(),
            vec!["invalid.event".to_string()],
        );
        assert!(result.is_err());

        // Test empty events
        let result = Webhook::new(
            team_id,
            created_by,
            "https://example.com/webhook".to_string(),
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_webhook_delivery_creation() {
        let webhook_id = Uuid::new_v4();
        let job_id = Some(Uuid::new_v4());
        let event_type = "job.completed".to_string();
        let payload = json!({"job_id": job_id, "status": "completed"});

        let delivery =
            WebhookDelivery::new(webhook_id, job_id, event_type.clone(), payload.clone());

        assert_eq!(delivery.webhook_id, webhook_id);
        assert_eq!(delivery.job_id, job_id);
        assert_eq!(delivery.event_type, event_type);
        assert_eq!(delivery.payload.0, payload);
        assert_eq!(delivery.status, WebhookDeliveryStatus::Pending);
        assert_eq!(delivery.attempts, 0);
        assert_eq!(delivery.max_attempts, 5);
    }

    #[test]
    fn test_usage_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let period = "2025-01".to_string();

        let usage = Usage::new(owner.clone(), period.clone()).unwrap();

        assert_eq!(usage.owner_urn().unwrap(), owner);
        assert_eq!(usage.period, period);
        assert_eq!(usage.renders_count, 0);
        assert_eq!(usage.credits_used, 0);
        assert_eq!(usage.net_credits(), 0);
    }

    #[test]
    fn test_usage_period_validation() {
        let owner = Urn::user(Uuid::new_v4());

        // Invalid period formats
        assert!(Usage::new(owner.clone(), "2025".to_string()).is_err());
        assert!(Usage::new(owner.clone(), "2025-1".to_string()).is_err());
        assert!(Usage::new(owner.clone(), "2025-13".to_string()).is_err());
        assert!(Usage::new(owner.clone(), "25-01".to_string()).is_err());

        // Valid period
        assert!(Usage::new(owner, "2025-01".to_string()).is_ok());
    }

    #[test]
    fn test_system_asset_creation() {
        let category = SystemAssetCategory::Sfx;
        let name = "whoosh_01".to_string();
        let description = "Wind whoosh sound effect".to_string();
        let s3_key = "system/sfx/whoosh_01.wav".to_string();
        let content_type = "audio/wav".to_string();
        let size_bytes = 2048;
        let tags = vec!["wind".to_string(), "whoosh".to_string()];

        let asset = SystemAsset::new(
            category,
            name.clone(),
            description.clone(),
            s3_key.clone(),
            content_type.clone(),
            size_bytes,
            None,
            tags.clone(),
        )
        .unwrap();

        assert_eq!(asset.id, format!("asset_sfx_{}", name));
        assert_eq!(asset.name, name);
        assert_eq!(asset.description, description);
        assert_eq!(asset.tags.0, tags);
    }

    #[test]
    fn test_system_asset_id_validation() {
        // Test invalid name (uppercase)
        let result = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "INVALID_NAME".to_string(),
            "Description".to_string(),
            "key".to_string(),
            "audio/wav".to_string(),
            1024,
            None,
            vec![],
        );
        assert!(result.is_err());

        // Test description too long
        let result = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "valid_name".to_string(),
            "a".repeat(501),
            "key".to_string(),
            "audio/wav".to_string(),
            1024,
            None,
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_job_status_terminal() {
        assert!(!JobStatus::Queued.is_terminal());
        assert!(!JobStatus::Processing.is_terminal());
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Canceled.is_terminal());
    }

    #[test]
    fn test_job_refund_calculation() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Set 40% progress
        job.update_progress(40.0).unwrap();

        // System error: Full refund
        let system_refund = job.calculate_refund(JobFailureType::System);
        assert_eq!(system_refund, 100);

        // Timeout: Full refund
        let timeout_refund = job.calculate_refund(JobFailureType::Timeout);
        assert_eq!(timeout_refund, 100);

        // Validation error: Partial refund based on remaining work
        // 60% remaining = 60 credits refunded
        let validation_refund = job.calculate_refund(JobFailureType::Validation);
        assert_eq!(validation_refund, 60);

        // Cancellation: Partial refund with 10% fee
        // 60% remaining × 0.9 = 54 credits refunded
        let cancel_refund = job.calculate_refund(JobFailureType::Canceled);
        assert_eq!(cancel_refund, 54);
    }

    #[test]
    fn test_job_progress_methods() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Initially 0% progress
        assert_eq!(job.get_progress_percent(), 0.0);

        // Update progress
        job.update_progress(25.5).unwrap();
        assert_eq!(job.get_progress_percent(), 25.5);

        // Progress bounds validation
        assert!(job.update_progress(-1.0).is_err());
        assert!(job.update_progress(101.0).is_err());

        // Progress clamped to bounds in getter
        job.progress = Json(json!({"percent": 150.0}));
        assert_eq!(job.get_progress_percent(), 100.0);

        job.progress = Json(json!({"percent": -50.0}));
        assert_eq!(job.get_progress_percent(), 0.0);
    }

    #[test]
    fn test_job_fail_with_automatic_refund() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Start the job
        job.start().unwrap();
        assert_eq!(job.status, JobStatus::Processing);

        // Set some progress
        job.update_progress(30.0).unwrap();

        // Fail with system error
        job.fail(json!({"error": "GPU crashed"}), JobFailureType::System)
            .unwrap();

        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.failure_type, Some(JobFailureType::System));
        assert_eq!(job.credits_refunded, 100); // Full refund for system error
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_job_cancel_with_automatic_refund() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Start the job
        job.start().unwrap();
        assert_eq!(job.status, JobStatus::Processing);

        // Set some progress (20%)
        job.update_progress(20.0).unwrap();

        // Cancel the job
        job.cancel().unwrap();

        assert_eq!(job.status, JobStatus::Canceled);
        assert_eq!(job.failure_type, Some(JobFailureType::Canceled));

        // 80% remaining work × 0.9 (10% cancellation fee) = 72 credits refunded
        assert_eq!(job.credits_refunded, 72);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_job_refund_edge_cases() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // 0% progress - full refund minus fee for cancellation
        job.update_progress(0.0).unwrap();
        let cancel_refund = job.calculate_refund(JobFailureType::Canceled);
        assert_eq!(cancel_refund, 90); // 100% × 0.9

        // 100% progress - no refund for any failure type except system/timeout
        job.update_progress(100.0).unwrap();

        assert_eq!(job.calculate_refund(JobFailureType::System), 100); // Still full
        assert_eq!(job.calculate_refund(JobFailureType::Timeout), 100); // Still full
        assert_eq!(job.calculate_refund(JobFailureType::Validation), 0); // No remaining work
        assert_eq!(job.calculate_refund(JobFailureType::Canceled), 0); // No remaining work

        // Test with no credits charged
        let user_owner2 = Urn::user(user_id);
        let mut free_job = Job::new(user_owner2, user_id, None, json!({}), 0, None).unwrap();
        free_job.update_progress(50.0).unwrap();
        assert_eq!(free_job.calculate_refund(JobFailureType::System), 0);
        assert_eq!(free_job.calculate_refund(JobFailureType::Canceled), 0);
    }

    #[test]
    fn test_refund_precision_edge_cases() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test cases that verify correct FLOOR behavior according to spec
        let precision_test_cases = vec![
            // (credits, progress, expected_validation_refund, expected_cancel_refund, description)
            (101, 33.33, 67, 60, "Odd credits with fractional progress"),
            (99, 50.5, 49, 44, "Even credits with fractional progress"),
            (1, 75.0, 0, 0, "Single credit edge case"),
            (1000, 0.1, 999, 899, "Large amount with tiny progress"),
            (
                5,
                33.33,
                3,
                3,
                "Small amount with fractional progress - CORRECTED",
            ),
            (33, 33.33, 22, 19, "Matching credit amount and progress"),
            (1, 1.0, 0, 0, "Minimal progress on single credit"),
            (999, 99.9, 0, 0, "Near-complete progress"),
            (
                1001,
                66.67,
                333,
                300,
                "Large odd amount with common fraction",
            ),
        ];

        for (credits, progress, expected_validation, expected_cancel, description) in
            precision_test_cases
        {
            let mut job =
                Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
            job.update_progress(progress).unwrap();

            // Test validation refund
            let validation_refund = job.calculate_refund(JobFailureType::Validation);
            assert_eq!(
                validation_refund, expected_validation,
                "Validation refund mismatch for {}: {} credits at {}% progress",
                description, credits, progress
            );

            // Test cancellation refund
            let cancel_refund = job.calculate_refund(JobFailureType::Canceled);
            assert_eq!(
                cancel_refund, expected_cancel,
                "Cancellation refund mismatch for {}: {} credits at {}% progress",
                description, credits, progress
            );
        }
    }

    #[test]
    fn test_refund_boundary_conditions() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test zero credits
        let mut zero_job = Job::new(user_owner.clone(), user_id, None, json!({}), 0, None).unwrap();
        zero_job.update_progress(50.0).unwrap();
        assert_eq!(zero_job.calculate_refund(JobFailureType::System), 0);
        assert_eq!(zero_job.calculate_refund(JobFailureType::Timeout), 0);
        assert_eq!(zero_job.calculate_refund(JobFailureType::Validation), 0);
        assert_eq!(zero_job.calculate_refund(JobFailureType::Canceled), 0);

        // Test single credit with various progress values
        let single_credit_cases = vec![
            (0.0, 1, 0),  // 0% progress: full validation refund, 90% cancel refund
            (10.0, 0, 0), // 10% progress: 90% validation refund (0.9 → 0), 81% cancel refund (0.729 → 0)
            (50.0, 0, 0), // 50% progress: 50% validation refund (0.5 → 0), 45% cancel refund (0.45 → 0)
            (90.0, 0, 0), // 90% progress: 10% validation refund (0.1 → 0), 9% cancel refund (0.09 → 0)
            (99.0, 0, 0), // 99% progress: 1% validation refund (0.01 → 0), 0.9% cancel refund (0.009 → 0)
        ];

        for (progress, expected_validation, expected_cancel) in single_credit_cases {
            let mut single_job =
                Job::new(user_owner.clone(), user_id, None, json!({}), 1, None).unwrap();
            single_job.update_progress(progress).unwrap();

            assert_eq!(
                single_job.calculate_refund(JobFailureType::Validation),
                expected_validation,
                "Single credit validation refund at {}% progress",
                progress
            );

            assert_eq!(
                single_job.calculate_refund(JobFailureType::Canceled),
                expected_cancel,
                "Single credit cancellation refund at {}% progress",
                progress
            );
        }

        // Test maximum safe integer values
        let max_safe_credits = 1_000_000_000; // Large but safe for i32 math
        let mut large_job =
            Job::new(user_owner, user_id, None, json!({}), max_safe_credits, None).unwrap();

        // Test with small progress to ensure no overflow
        large_job.update_progress(1.0).unwrap();
        let large_validation_refund = large_job.calculate_refund(JobFailureType::Validation);
        let large_cancel_refund = large_job.calculate_refund(JobFailureType::Canceled);

        // With 1% progress: 99% should be refunded for validation, 89.1% for cancellation
        assert_eq!(large_validation_refund, 990_000_000); // 99% of 1B
        assert_eq!(large_cancel_refund, 891_000_000); // 99% * 90% of 1B
    }

    #[test]
    fn test_cancellation_minimum_charge_enforcement() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test cases where 10% minimum charge should be enforced
        let enforcement_cases = vec![
            // (credits, progress, expected_refund, expected_charge, description)
            (
                100,
                0.0,
                90,
                10,
                "Zero progress should enforce minimum 10% charge",
            ),
            (100, 5.0, 85, 15, "Low progress within normal range"),
            (100, 1.0, 89, 11, "Tiny progress should still work normally"),
            (50, 0.0, 45, 5, "Half credits with zero progress"),
            (10, 0.0, 9, 1, "Small amount with zero progress"),
            (1000, 0.1, 899, 101, "Large amount with minimal progress"),
        ];

        for (credits, progress, expected_refund, expected_charge, description) in enforcement_cases
        {
            let mut job =
                Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
            job.update_progress(progress).unwrap();

            let actual_refund = job.calculate_refund(JobFailureType::Canceled);
            let actual_charge = credits - actual_refund;

            assert_eq!(
                actual_refund, expected_refund,
                "Refund mismatch: {}",
                description
            );
            assert_eq!(
                actual_charge, expected_charge,
                "Charge mismatch: {}",
                description
            );

            // Verify minimum charge constraint
            let min_charge = (credits as f64 * 0.1).ceil() as i32;
            assert!(
                actual_charge >= min_charge || actual_charge == credits,
                "Minimum 10% charge not enforced for {}: actual charge {} < minimum {}",
                description,
                actual_charge,
                min_charge
            );
        }
    }

    #[test]
    fn test_refund_calculation_consistency() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test that refund calculations are consistent across multiple calls
        let mut job = Job::new(user_owner, user_id, None, json!({}), 150, None).unwrap();
        job.update_progress(33.33).unwrap();

        // Call refund calculation multiple times - should be deterministic
        let refunds: Vec<_> = (0..10)
            .map(|_| job.calculate_refund(JobFailureType::Validation))
            .collect();

        // All refunds should be identical
        assert!(
            refunds.iter().all(|&r| r == refunds[0]),
            "Refund calculations should be deterministic"
        );

        // Same for cancellation refunds
        let cancel_refunds: Vec<_> = (0..10)
            .map(|_| job.calculate_refund(JobFailureType::Canceled))
            .collect();

        assert!(
            cancel_refunds.iter().all(|&r| r == cancel_refunds[0]),
            "Cancellation refunds should be deterministic"
        );
    }

    #[test]
    fn test_refund_mathematical_properties() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test mathematical properties that should hold for refund calculations
        for credits in [1, 10, 100, 1000] {
            for progress in [0.0, 25.0, 50.0, 75.0, 100.0] {
                let mut job =
                    Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
                job.update_progress(progress).unwrap();

                // Property 1: System/timeout refunds should always equal charged amount
                assert_eq!(job.calculate_refund(JobFailureType::System), credits);
                assert_eq!(job.calculate_refund(JobFailureType::Timeout), credits);

                // Property 2: Refunds should never exceed charged amount
                let validation_refund = job.calculate_refund(JobFailureType::Validation);
                let cancel_refund = job.calculate_refund(JobFailureType::Canceled);

                assert!(
                    validation_refund <= credits,
                    "Validation refund {} exceeds credits {} for {}% progress",
                    validation_refund,
                    credits,
                    progress
                );
                assert!(
                    cancel_refund <= credits,
                    "Cancellation refund {} exceeds credits {} for {}% progress",
                    cancel_refund,
                    credits,
                    progress
                );

                // Property 3: Cancellation refund should never exceed validation refund
                assert!(cancel_refund <= validation_refund,
                        "Cancellation refund {} exceeds validation refund {} for {} credits at {}% progress",
                        cancel_refund, validation_refund, credits, progress);

                // Property 4: At 100% progress, validation and cancellation refunds should be 0
                if progress == 100.0 {
                    assert_eq!(validation_refund, 0);
                    assert_eq!(cancel_refund, 0);
                }

                // Property 5: At 0% progress, validation refund should equal credits
                if progress == 0.0 {
                    assert_eq!(validation_refund, credits);
                    // Cancellation should be 90% of credits (or less due to integer math)
                    assert!(cancel_refund <= (credits as f64 * 0.9).floor() as i32);
                }
            }
        }
    }

    #[test]
    fn test_progress_percentage_edge_cases() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Test various progress percentage edge cases
        let progress_cases = vec![
            (0.0, "Zero progress"),
            (0.01, "Minimal progress"),
            (33.33, "Common fraction (1/3)"),
            (66.67, "Common fraction (2/3)"),
            (99.99, "Near completion"),
            (100.0, "Full completion"),
        ];

        for (progress, description) in progress_cases {
            job.update_progress(progress).unwrap();

            // Verify progress is set correctly
            assert!(
                (job.get_progress_percent() - progress).abs() < 0.01,
                "Progress not set correctly for {}: expected {}, got {}",
                description,
                progress,
                job.get_progress_percent()
            );

            // Verify refunds are calculated correctly
            let validation_refund = job.calculate_refund(JobFailureType::Validation);
            let cancel_refund = job.calculate_refund(JobFailureType::Canceled);

            // Basic sanity checks
            assert!(
                validation_refund >= 0,
                "Validation refund negative for {}",
                description
            );
            assert!(
                cancel_refund >= 0,
                "Cancellation refund negative for {}",
                description
            );
            assert!(
                validation_refund <= 100,
                "Validation refund too high for {}",
                description
            );
            assert!(
                cancel_refund <= 100,
                "Cancellation refund too high for {}",
                description
            );
        }
    }

    #[test]
    fn test_floating_point_precision_issues_that_fail() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test case 1: Float precision loss causing incorrect FLOOR behavior
        // This test will FAIL with current implementation due to float imprecision
        let mut job1 = Job::new(user_owner.clone(), user_id, None, json!({}), 7, None).unwrap();
        job1.update_progress(42.857142857142854).unwrap(); // 3/7 as decimal with precision issues

        // Expected: 7 * (1 - 0.42857142857142854) * 0.9 = 7 * 0.57142857142857146 * 0.9 = 3.5999999999999996
        // FLOOR(3.5999999999999996) should be 3
        // But float imprecision might cause this to be 4.0 in some cases, leading to wrong result
        let cancel_refund = job1.calculate_refund(JobFailureType::Canceled);

        // This assertion may PASS or FAIL depending on floating point precision
        // The point is to show that float arithmetic is unreliable
        println!(
            "7 credits at 42.857% progress: cancel_refund = {}",
            cancel_refund
        );

        // Test case 2: Accumulated precision errors with repeated calculations
        let mut job2 = Job::new(user_owner.clone(), user_id, None, json!({}), 1000, None).unwrap();

        // Progress that when converted through float operations introduces error
        let tricky_progress = 1.0 / 3.0 * 100.0; // 33.333333... with float precision issues
        job2.update_progress(tricky_progress).unwrap();

        let validation_refund = job2.calculate_refund(JobFailureType::Validation);
        // Expected: 1000 * (1 - 0.3333333333333333) = 1000 * 0.6666666666666667 = 666.6666666666667
        // FLOOR should give 666, but float precision might give different result
        println!(
            "1000 credits at {:.15}% progress: validation_refund = {}",
            tricky_progress, validation_refund
        );

        // The issue is that these results are not deterministic across platforms due to float precision
    }

    #[test]
    fn test_missing_minimum_charge_enforcement_failures() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // This test demonstrates the missing minimum charge enforcement from the spec
        // Current implementation does NOT enforce the MIN(refund, credits * 0.9) constraint

        let test_cases = vec![
            // Cases where current implementation might violate minimum charge
            (100, 0.0), // Should refund max 90, charge min 10
            (50, 0.0),  // Should refund max 45, charge min 5
        ];

        for (credits, progress) in test_cases {
            let mut job =
                Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
            job.update_progress(progress).unwrap();

            let refund = job.calculate_refund(JobFailureType::Canceled);
            let charge = credits - refund;

            // Current implementation calculation: credits * (1 - progress/100) * 0.9
            // For 0% progress: credits * 1.0 * 0.9 = credits * 0.9 ✓ This actually works

            // But spec also says: MIN(calculated_refund, credits * 0.9)
            // This constraint is missing from current implementation
            let max_allowed_refund = (credits as f64 * 0.9) as i32;

            println!(
                "Credits: {}, Progress: {}%, Refund: {}, Charge: {}, Max allowed refund: {}",
                credits, progress, refund, charge, max_allowed_refund
            );

            // This assertion should pass with current implementation for 0% progress
            // but demonstrates the missing constraint for other edge cases
        }
    }

    #[test]
    fn test_truncation_vs_rounding_precision_loss() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test cases designed to show truncation vs proper rounding issues
        let truncation_cases = vec![
            // (credits, progress) where float calculation results in X.999... that gets truncated
            (13, 23.08), // Should give specific problematic float result
            (37, 13.51), // Another case with precision issues
            (83, 9.64),  // Designed to expose truncation problems
        ];

        for (credits, progress) in truncation_cases {
            let mut job =
                Job::new(user_owner.clone(), user_id, None, json!({}), credits, None).unwrap();
            job.update_progress(progress).unwrap();

            let validation_refund = job.calculate_refund(JobFailureType::Validation);
            let cancel_refund = job.calculate_refund(JobFailureType::Canceled);

            // Calculate what the result SHOULD be with proper FLOOR
            let remaining_work = 1.0 - (progress / 100.0);
            let expected_validation = (credits as f64 * remaining_work).floor() as i32;
            let expected_cancel = (credits as f64 * remaining_work * 0.9).floor() as i32;

            println!("Credits: {}, Progress: {}%", credits, progress);
            println!("  Remaining work: {:.10}", remaining_work);
            println!(
                "  Current validation refund: {}, Expected: {}",
                validation_refund, expected_validation
            );
            println!(
                "  Current cancel refund: {}, Expected: {}",
                cancel_refund, expected_cancel
            );

            // These might pass or fail depending on floating point precision
            // The key insight is that float arithmetic makes results unpredictable
        }
    }

    #[test]
    fn test_deterministic_behavior_failures() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);

        // Test that demonstrates non-deterministic behavior with current float implementation
        let mut job = Job::new(user_owner, user_id, None, json!({}), 111, None).unwrap();

        // Use a progress value that causes precision issues
        let problematic_progress = 100.0 / 3.0; // 33.333...
        job.update_progress(problematic_progress).unwrap();

        // Run the same calculation multiple times
        let refunds: Vec<i32> = (0..100)
            .map(|_| job.calculate_refund(JobFailureType::Validation))
            .collect();

        // With current float implementation, all results should be the same
        // But this demonstrates the potential for non-deterministic behavior
        let all_same = refunds.iter().all(|&r| r == refunds[0]);

        println!("111 credits at {:.15}% progress:", problematic_progress);
        println!(
            "All {} calculations gave same result: {}",
            refunds.len(),
            all_same
        );
        println!("Result: {} credits", refunds[0]);

        // This test mainly serves to document the precision concern
        // The real issue is that float arithmetic is inherently imprecise for financial calculations
    }

    #[test]
    fn test_state_machine_with_automatic_refunds() {
        let user_id = Uuid::new_v4();
        let user_owner = Urn::user(user_id);
        let mut job = Job::new(user_owner, user_id, None, json!({}), 100, None).unwrap();

        // Start the job and set some progress
        job.start().unwrap();
        job.update_progress(40.0).unwrap();

        // Test automatic refund calculation on failure
        job.fail(json!({"error": "System failure"}), JobFailureType::System)
            .unwrap();

        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.failure_type, Some(JobFailureType::System));
        assert_eq!(job.credits_refunded, 100); // Full refund for system error
        assert!(job.completed_at.is_some());

        // Test cancellation with automatic refund
        let user_owner2 = Urn::user(user_id);
        let mut job2 = Job::new(user_owner2, user_id, None, json!({}), 100, None).unwrap();
        job2.start().unwrap();
        job2.update_progress(30.0).unwrap();

        job2.cancel().unwrap();

        assert_eq!(job2.status, JobStatus::Canceled);
        assert_eq!(job2.failure_type, Some(JobFailureType::Canceled));
        // 70% remaining × 0.9 = 63 credits refunded
        assert_eq!(job2.credits_refunded, 63);
        assert!(job2.completed_at.is_some());
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
    fn test_invitation_email_max_length_boundary() {
        // 254-char email should pass (valid per RFC 5321)
        let local_part = "a".repeat(63);
        let domain = format!("{}.com", "b".repeat(186));
        let email_254 = format!("{}@{}", local_part, domain);
        assert!(email_254.len() <= 255);
        // This should be valid since it contains '@' and is <= 255 chars
        let result = Invitation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            email_254,
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
    fn test_slug_consecutive_hyphens_valid() {
        // Consecutive hyphens are allowed by current validation (no rule against them)
        assert!(Team::validate_slug("a--b").is_ok());
    }

    #[test]
    fn test_slug_unicode_rejected() {
        assert!(Team::validate_slug("café").is_err());
        assert!(Team::validate_slug("日本語").is_err());
        assert!(Team::validate_slug("team-αβ").is_err());
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
    fn test_user_validate_email_or_conditions() {
        // Kill: replace || with && in email check
        // Test email without '@' but within length limit -> should fail
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "noemailatall".to_string(),
            name: None,
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(user.validate().is_err());

        // Test email with '@' but too long -> should fail
        user.email = format!("{}@example.com", "a".repeat(250));
        assert!(user.email.len() > 255);
        assert!(user.validate().is_err());

        // Test email with '@' and within length limit -> should pass
        user.email = "ok@example.com".to_string();
        assert!(user.validate().is_ok());
    }

    #[test]
    fn test_user_validate_email_len_boundary() {
        // Kill: replace > with == and >= (email.len() > 255)
        let now = Utc::now();
        let mut user = User {
            id: Uuid::new_v4(),
            email: "ok@example.com".to_string(),
            name: None,
            avatar_url: None,
            tier: UserTier::Starter,
            credits: 0,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: now,
            updated_at: now,
        };

        // "@example.com" is 12 chars, so local_part needs to be 243 for 255 total
        // 255-char email with '@' should be valid
        let local_part = "a".repeat(243);
        user.email = format!("{}@example.com", local_part);
        assert_eq!(user.email.len(), 255);
        assert!(user.validate().is_ok());

        // 256-char email should be invalid
        let local_part = "a".repeat(244);
        user.email = format!("{}@example.com", local_part);
        assert_eq!(user.email.len(), 256);
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

    // ========================================================================
    // Mutant-killing tests: ProjectStatus
    // ========================================================================

    #[test]
    fn test_project_status_is_terminal_true_false() {
        // Kill: replace is_terminal with true and false
        // Project has no terminal states per spec
        assert!(!ProjectStatus::Draft.is_terminal());
        assert!(!ProjectStatus::Rendering.is_terminal());
        assert!(!ProjectStatus::Completed.is_terminal());
        assert!(!ProjectStatus::Archived.is_terminal());
    }

    #[test]
    fn test_project_status_from_state_all_variants() {
        // Kill: replace from_state with Default::default()
        // Default is Draft, so test non-Draft variants
        assert_eq!(
            ProjectStatus::from_state(ProjectState::Rendering),
            ProjectStatus::Rendering
        );
        assert_eq!(
            ProjectStatus::from_state(ProjectState::Completed),
            ProjectStatus::Completed
        );
        assert_eq!(
            ProjectStatus::from_state(ProjectState::Archived),
            ProjectStatus::Archived
        );
        assert_eq!(
            ProjectStatus::from_state(ProjectState::Draft),
            ProjectStatus::Draft
        );
    }

    #[test]
    fn test_project_status_valid_transitions_not_empty() {
        // Kill: replace valid_transitions with vec![] and vec![Default::default()]
        let draft_transitions = ProjectStatus::Draft.valid_transitions();
        assert!(!draft_transitions.is_empty());
        assert!(draft_transitions.contains(&ProjectStatus::Rendering));
        assert!(draft_transitions.contains(&ProjectStatus::Archived));

        let rendering_transitions = ProjectStatus::Rendering.valid_transitions();
        assert!(!rendering_transitions.is_empty());
        assert!(rendering_transitions.contains(&ProjectStatus::Completed));

        let completed_transitions = ProjectStatus::Completed.valid_transitions();
        assert!(!completed_transitions.is_empty());
        assert!(completed_transitions.contains(&ProjectStatus::Archived));

        let archived_transitions = ProjectStatus::Archived.valid_transitions();
        assert!(!archived_transitions.is_empty());
        assert!(archived_transitions.contains(&ProjectStatus::Draft));
    }

    // ========================================================================
    // Mutant-killing tests: Project
    // ========================================================================

    #[test]
    fn test_project_validate_returns_err_on_invalid() {
        // Kill: replace validate with Ok(())
        let now = Utc::now();
        let project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "a".repeat(201),
            status: ProjectStatus::Draft,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        assert!(project.validate().is_err());
    }

    #[test]
    fn test_project_validate_name_len_boundary() {
        // Kill: replace > with ==, <, >= (name len)
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "a".repeat(200),
            status: ProjectStatus::Draft,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        // 200 chars should be valid
        assert!(project.validate().is_ok());
        // 201 chars should be invalid
        project.name = "a".repeat(201);
        assert!(project.validate().is_err());
        // 199 chars should be valid
        project.name = "a".repeat(199);
        assert!(project.validate().is_ok());
    }

    #[test]
    fn test_project_start_render_changes_status() {
        // Kill: replace start_render with Ok(())
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Draft,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        project.start_render().unwrap();
        assert_eq!(project.status, ProjectStatus::Rendering);
    }

    #[test]
    fn test_project_on_job_completed_changes_status() {
        // Kill: replace on_job_completed with Ok(())
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Rendering,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        project.on_job_completed().unwrap();
        assert_eq!(project.status, ProjectStatus::Completed);
    }

    #[test]
    fn test_project_on_job_failed_changes_status() {
        // Kill: replace on_job_failed with Ok(())
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Rendering,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        project.on_job_failed().unwrap();
        assert_eq!(project.status, ProjectStatus::Draft);
    }

    #[test]
    fn test_project_on_job_canceled_changes_status() {
        // Kill: replace on_job_canceled with Ok(())
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Rendering,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        project.on_job_canceled().unwrap();
        assert_eq!(project.status, ProjectStatus::Draft);
    }

    #[test]
    fn test_project_archive_changes_status() {
        // Kill: replace archive with Ok(())
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Draft,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        project.archive().unwrap();
        assert_eq!(project.status, ProjectStatus::Archived);
    }

    #[test]
    fn test_project_unarchive_changes_status() {
        // Kill: replace unarchive with Ok(())
        let now = Utc::now();
        let mut project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Archived,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        project.unarchive().unwrap();
        assert_eq!(project.status, ProjectStatus::Draft);
    }

    #[test]
    fn test_project_can_transition_true_false() {
        // Kill: replace can_transition with true and false
        let now = Utc::now();
        let project = Project {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            name: "Test".to_string(),
            status: ProjectStatus::Draft,
            spec: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        // Draft can render
        assert!(project.can_transition(&ProjectEvent::Render));
        // Draft cannot unarchive
        assert!(!project.can_transition(&ProjectEvent::Unarchive));
        // Draft cannot complete a job
        assert!(!project.can_transition(&ProjectEvent::JobCompleted));
    }

    // ========================================================================
    // Mutant-killing tests: JobStatus::valid_transitions
    // ========================================================================

    #[test]
    fn test_job_status_valid_transitions_not_empty() {
        // Kill: replace with vec![] and vec![Default::default()]
        let queued_transitions = JobStatus::Queued.valid_transitions();
        assert!(!queued_transitions.is_empty());
        assert!(queued_transitions.contains(&JobStatus::Processing));
        assert!(queued_transitions.contains(&JobStatus::Canceled));

        let processing_transitions = JobStatus::Processing.valid_transitions();
        assert!(!processing_transitions.is_empty());
        assert!(processing_transitions.contains(&JobStatus::Completed));
        assert!(processing_transitions.contains(&JobStatus::Failed));
        assert!(processing_transitions.contains(&JobStatus::Canceled));

        // Terminal states have no transitions
        assert!(JobStatus::Completed.valid_transitions().is_empty());
        assert!(JobStatus::Failed.valid_transitions().is_empty());
        assert!(JobStatus::Canceled.valid_transitions().is_empty());
    }

    // ========================================================================
    // Mutant-killing tests: Job
    // ========================================================================

    #[test]
    fn test_job_is_ephemeral_with_project() {
        // Kill: replace is_ephemeral with true
        let user_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let owner = Urn::team(team_id);
        let project_id = Some(Uuid::new_v4());
        let job = Job::new(owner, user_id, project_id, json!({}), 100, None).unwrap();
        assert!(!job.is_ephemeral());

        // Without project
        let owner2 = Urn::user(user_id);
        let job2 = Job::new(owner2, user_id, None, json!({}), 100, None).unwrap();
        assert!(job2.is_ephemeral());
    }

    #[test]
    fn test_job_net_credits_calculation() {
        // Kill: replace net_credits with 0, 1, -1; replace - with +, /
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let mut job = Job::new(owner, user_id, None, json!({}), 100, None).unwrap();
        assert_eq!(job.net_credits(), 100); // 100 - 0

        job.credits_refunded = 30;
        assert_eq!(job.net_credits(), 70); // 100 - 30

        job.credits_refunded = 100;
        assert_eq!(job.net_credits(), 0); // 100 - 100

        // Use specific values to detect + vs - mutation
        job.credits_charged = 50;
        job.credits_refunded = 20;
        assert_eq!(job.net_credits(), 30); // 50 - 20, NOT 50 + 20 = 70
    }

    #[test]
    fn test_job_can_transition_true_false() {
        // Kill: replace can_transition with true and false
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let job = Job::new(owner, user_id, None, json!({}), 100, None).unwrap();
        // Queued can start
        assert!(job.can_transition(&JobEvent::WorkerPicksUp));
        // Queued cannot complete
        assert!(!job.can_transition(&JobEvent::Success));
    }

    #[test]
    fn test_job_validate_refunded_gt_charged() {
        // Kill: replace > with >= (refunded > charged)
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let mut job = Job::new(owner, user_id, None, json!({}), 100, None).unwrap();

        // refunded == charged should be valid
        job.credits_refunded = 100;
        assert!(job.validate().is_ok());

        // refunded > charged should be invalid
        job.credits_refunded = 101;
        assert!(job.validate().is_err());
    }

    #[test]
    fn test_job_validate_negative_credits_or_conditions() {
        // Kill: replace || with &&; replace < with > for refunded and charged
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let now = Utc::now();

        // Negative refunded only (not negative charged) -> should fail
        let job1 = Job {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            triggered_by: user_id,
            project_id: None,
            status: JobStatus::Queued,
            spec_snapshot: Json(json!({})),
            options: Json(json!({})),
            progress: Json(json!({})),
            output: None,
            output_size_bytes: None,
            error: None,
            credits_charged: 100,
            failure_type: None,
            credits_refunded: -1,
            idempotency_key: None,
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(job1.validate().is_err());

        // Negative charged only (not negative refunded) -> should fail
        let job2 = Job {
            credits_charged: -1,
            credits_refunded: 0,
            ..job1.clone()
        };
        assert!(job2.validate().is_err());

        // Both zero -> should pass
        let job3 = Job {
            credits_charged: 0,
            credits_refunded: 0,
            ..job1.clone()
        };
        assert!(job3.validate().is_ok());
    }

    #[test]
    fn test_job_validate_failure_type_match_arms() {
        // Kill: delete match arm (Failed|Canceled, None) and (Completed, Some(_))
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let now = Utc::now();

        // Failed job without failure_type -> should fail
        let job_failed_no_type = Job {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            triggered_by: user_id,
            project_id: None,
            status: JobStatus::Failed,
            spec_snapshot: Json(json!({})),
            options: Json(json!({})),
            progress: Json(json!({})),
            output: None,
            output_size_bytes: None,
            error: Some(Json(json!({"msg": "error"}))),
            credits_charged: 100,
            failure_type: None,
            credits_refunded: 0,
            idempotency_key: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
            updated_at: now,
        };
        assert!(job_failed_no_type.validate().is_err());

        // Canceled job without failure_type -> should fail
        let job_canceled_no_type = Job {
            status: JobStatus::Canceled,
            ..job_failed_no_type.clone()
        };
        assert!(job_canceled_no_type.validate().is_err());

        // Completed job WITH failure_type -> should fail
        let job_completed_with_type = Job {
            status: JobStatus::Completed,
            output: Some(Json(json!({"url": "test"}))),
            error: None,
            failure_type: Some(JobFailureType::System),
            ..job_failed_no_type.clone()
        };
        assert!(job_completed_with_type.validate().is_err());
    }

    #[test]
    fn test_job_validate_cancellation_charge_arithmetic() {
        // Kill: arithmetic mutations (*, /, -, <) in cancellation charge calc
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let now = Utc::now();

        // Cancellation with 100 credits: min_charge = 10, max refund = 90
        let mut job = Job {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            triggered_by: user_id,
            project_id: None,
            status: JobStatus::Canceled,
            spec_snapshot: Json(json!({})),
            options: Json(json!({})),
            progress: Json(json!({})),
            output: None,
            output_size_bytes: None,
            error: None,
            credits_charged: 100,
            failure_type: Some(JobFailureType::Canceled),
            credits_refunded: 90, // exactly 90% refund
            idempotency_key: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
            updated_at: now,
        };
        // actual_charge = 100 - 90 = 10, min_charge = 10 -> valid
        assert!(job.validate().is_ok());

        // Refund 91 -> charge 9 < min_charge 10 -> invalid
        job.credits_refunded = 91;
        assert!(job.validate().is_err());
    }

    #[test]
    fn test_job_max_size_bytes_arithmetic() {
        // Kill: replace * with + (MAX_SIZE_BYTES calc - 2 mutants)
        // MAX_SIZE_BYTES = 50 * 1024 * 1024 = 52428800
        assert_eq!(AssetFile::MAX_SIZE_BYTES, 50 * 1024 * 1024);
        assert_eq!(AssetFile::MAX_SIZE_BYTES, 52_428_800);
    }

    // ========================================================================
    // Mutant-killing tests: AssetFile::validate
    // ========================================================================

    #[test]
    fn test_asset_file_validate_returns_err_on_invalid() {
        // Kill: replace with Ok(())
        let now = Utc::now();
        let asset = AssetFile {
            id: Uuid::new_v4(),
            owner: Urn::user(Uuid::new_v4()).to_string(),
            uploaded_by: Uuid::new_v4(),
            project_id: None,
            filename: "".to_string(), // invalid
            s3_key: "key".to_string(),
            content_type: "image/jpeg".to_string(),
            size_bytes: 1024,
            status: AssetStatus::Pending,
            metadata: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        assert!(asset.validate().is_err());
    }

    #[test]
    fn test_asset_file_validate_or_conditions() {
        // Kill: replace || with && in filename check
        let now = Utc::now();
        // Empty filename (but not > 255) -> should fail
        let asset1 = AssetFile {
            id: Uuid::new_v4(),
            owner: Urn::user(Uuid::new_v4()).to_string(),
            uploaded_by: Uuid::new_v4(),
            project_id: None,
            filename: "".to_string(),
            s3_key: "key".to_string(),
            content_type: "image/jpeg".to_string(),
            size_bytes: 1024,
            status: AssetStatus::Pending,
            metadata: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        assert!(asset1.validate().is_err());

        // Filename > 255 (but not empty) -> should fail
        let asset2 = AssetFile {
            filename: "a".repeat(256),
            ..asset1.clone()
        };
        assert!(asset2.validate().is_err());
    }

    #[test]
    fn test_asset_file_validate_filename_len_boundary() {
        // Kill: replace > with ==, >= (filename len)
        let now = Utc::now();
        let base = AssetFile {
            id: Uuid::new_v4(),
            owner: Urn::user(Uuid::new_v4()).to_string(),
            uploaded_by: Uuid::new_v4(),
            project_id: None,
            filename: "a".repeat(255),
            s3_key: "key".to_string(),
            content_type: "image/jpeg".to_string(),
            size_bytes: 1024,
            status: AssetStatus::Pending,
            metadata: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        // 255 chars should be valid
        assert!(base.validate().is_ok());

        // 256 chars should be invalid
        let asset256 = AssetFile {
            filename: "a".repeat(256),
            ..base.clone()
        };
        assert!(asset256.validate().is_err());
    }

    #[test]
    fn test_asset_file_validate_size_boundary() {
        // Kill: replace > with ==, >= (size > MAX_SIZE_BYTES)
        let now = Utc::now();
        let mut asset = AssetFile {
            id: Uuid::new_v4(),
            owner: Urn::user(Uuid::new_v4()).to_string(),
            uploaded_by: Uuid::new_v4(),
            project_id: None,
            filename: "test.jpg".to_string(),
            s3_key: "key".to_string(),
            content_type: "image/jpeg".to_string(),
            size_bytes: AssetFile::MAX_SIZE_BYTES,
            status: AssetStatus::Pending,
            metadata: Json(json!({})),
            created_at: now,
            updated_at: now,
        };
        // Exactly MAX should be valid
        assert!(asset.validate().is_ok());

        // MAX + 1 should be invalid
        asset.size_bytes = AssetFile::MAX_SIZE_BYTES + 1;
        assert!(asset.validate().is_err());
    }

    // ========================================================================
    // Mutant-killing tests: Webhook::validate
    // ========================================================================

    #[test]
    fn test_webhook_validate_returns_err_on_invalid() {
        // Kill: replace with Ok(())
        let now = Utc::now();
        let webhook = Webhook {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            url: "http://not-https.com/webhook".to_string(), // invalid
            events: Json(vec!["job.completed".to_string()]),
            secret: "secret".to_string(), // pragma: allowlist secret // pragma: allowlist secret
            is_active: true,
            last_triggered_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(webhook.validate().is_err());
    }

    #[test]
    fn test_webhook_validate_url_not_starts_with_https() {
        // Kill: delete ! (url starts_with)
        let now = Utc::now();
        // HTTP URL -> should fail
        let webhook_http = Webhook {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            url: "http://example.com/webhook".to_string(),
            events: Json(vec!["job.completed".to_string()]),
            secret: "secret".to_string(), // pragma: allowlist secret
            is_active: true,
            last_triggered_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(webhook_http.validate().is_err());

        // HTTPS URL -> should pass
        let webhook_https = Webhook {
            url: "https://example.com/webhook".to_string(),
            ..webhook_http.clone()
        };
        assert!(webhook_https.validate().is_ok());
    }

    #[test]
    fn test_webhook_validate_url_len_boundary() {
        // Kill: replace > with ==, <, >= (url len)
        let now = Utc::now();
        // URL at 2048 chars should be valid
        let long_path = "a".repeat(2048 - "https://example.com/".len());
        let url_2048 = format!("https://example.com/{}", long_path);
        assert_eq!(url_2048.len(), 2048);
        let webhook_ok = Webhook {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            url: url_2048,
            events: Json(vec!["job.completed".to_string()]),
            secret: "secret".to_string(), // pragma: allowlist secret
            is_active: true,
            last_triggered_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(webhook_ok.validate().is_ok());

        // URL at 2049 chars should be invalid
        let long_path2 = "a".repeat(2049 - "https://example.com/".len());
        let url_2049 = format!("https://example.com/{}", long_path2);
        assert_eq!(url_2049.len(), 2049);
        let webhook_bad = Webhook {
            url: url_2049,
            ..webhook_ok.clone()
        };
        assert!(webhook_bad.validate().is_err());
    }

    #[test]
    fn test_webhook_validate_event_not_valid() {
        // Kill: delete ! (event validation)
        let now = Utc::now();
        // Invalid event -> should fail
        let webhook_bad = Webhook {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            url: "https://example.com/webhook".to_string(),
            events: Json(vec!["invalid.event".to_string()]),
            secret: "secret".to_string(), // pragma: allowlist secret
            is_active: true,
            last_triggered_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(webhook_bad.validate().is_err());

        // Valid event -> should pass
        let webhook_ok = Webhook {
            events: Json(vec!["job.completed".to_string()]),
            ..webhook_bad.clone()
        };
        assert!(webhook_ok.validate().is_ok());
    }

    // ========================================================================
    // Mutant-killing tests: WebhookDeliveryStatus
    // ========================================================================

    #[test]
    fn test_webhook_delivery_status_is_terminal_true_false() {
        // Kill: replace is_terminal with true and false
        assert!(!WebhookDeliveryStatus::Pending.is_terminal());
        assert!(!WebhookDeliveryStatus::Attempting.is_terminal());
        assert!(WebhookDeliveryStatus::Delivered.is_terminal());
        assert!(!WebhookDeliveryStatus::Retrying.is_terminal());
        assert!(WebhookDeliveryStatus::Failed.is_terminal());
    }

    #[test]
    fn test_webhook_delivery_status_from_state_all_variants() {
        // Kill: replace from_state with Default::default()
        // Default is Pending, so test non-Pending variants
        assert_eq!(
            WebhookDeliveryStatus::from_state(WebhookDeliveryState::Attempting),
            WebhookDeliveryStatus::Attempting
        );
        assert_eq!(
            WebhookDeliveryStatus::from_state(WebhookDeliveryState::Delivered),
            WebhookDeliveryStatus::Delivered
        );
        assert_eq!(
            WebhookDeliveryStatus::from_state(WebhookDeliveryState::Retrying),
            WebhookDeliveryStatus::Retrying
        );
        assert_eq!(
            WebhookDeliveryStatus::from_state(WebhookDeliveryState::Failed),
            WebhookDeliveryStatus::Failed
        );
    }

    #[test]
    fn test_webhook_delivery_status_valid_transitions_not_empty() {
        // Kill: replace valid_transitions with vec![] and vec![Default::default()]
        let pending_transitions = WebhookDeliveryStatus::Pending.valid_transitions();
        assert!(!pending_transitions.is_empty());
        assert!(pending_transitions.contains(&WebhookDeliveryStatus::Attempting));

        let attempting_transitions = WebhookDeliveryStatus::Attempting.valid_transitions();
        assert!(!attempting_transitions.is_empty());

        // Terminal states have no transitions
        assert!(WebhookDeliveryStatus::Delivered
            .valid_transitions()
            .is_empty());
        assert!(WebhookDeliveryStatus::Failed.valid_transitions().is_empty());
    }

    // ========================================================================
    // Mutant-killing tests: WebhookDelivery
    // ========================================================================

    #[test]
    fn test_webhook_delivery_validate_returns_err_on_invalid() {
        // Kill: replace validate with Ok(())
        let delivery = WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id: Uuid::new_v4(),
            job_id: None,
            event_type: "job.completed".to_string(),
            status: WebhookDeliveryStatus::Pending,
            payload: Json(json!({})),
            response_status: None,
            response_body: None,
            attempts: 6, // exceeds max
            max_attempts: 5,
            next_retry_at: None,
            delivered_at: None,
            created_at: Utc::now(),
        };
        assert!(delivery.validate().is_err());
    }

    #[test]
    fn test_webhook_delivery_validate_attempts_boundary() {
        // Kill: replace > with ==, <, >= (attempts > max)
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        delivery.max_attempts = 5;

        // attempts == max should be valid
        delivery.attempts = 5;
        assert!(delivery.validate().is_ok());

        // attempts = max + 1 should be invalid
        delivery.attempts = 6;
        assert!(delivery.validate().is_err());

        // attempts < max should be valid
        delivery.attempts = 4;
        assert!(delivery.validate().is_ok());
    }

    #[test]
    fn test_webhook_delivery_validate_delivered_status() {
        // Kill: replace && with ||; replace == with != (delivered status check)
        let now = Utc::now();
        // Delivered without delivered_at -> should fail
        let delivery_bad = WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id: Uuid::new_v4(),
            job_id: None,
            event_type: "job.completed".to_string(),
            status: WebhookDeliveryStatus::Delivered,
            payload: Json(json!({})),
            response_status: None,
            response_body: None,
            attempts: 1,
            max_attempts: 5,
            next_retry_at: None,
            delivered_at: None,
            created_at: now,
        };
        assert!(delivery_bad.validate().is_err());

        // Delivered with delivered_at -> should pass
        let delivery_ok = WebhookDelivery {
            delivered_at: Some(now),
            ..delivery_bad.clone()
        };
        assert!(delivery_ok.validate().is_ok());

        // Pending without delivered_at -> should pass (not delivered status)
        let delivery_pending = WebhookDelivery {
            status: WebhookDeliveryStatus::Pending,
            delivered_at: None,
            ..delivery_bad.clone()
        };
        assert!(delivery_pending.validate().is_ok());
    }

    #[test]
    fn test_webhook_delivery_start_attempt_changes_state() {
        // Kill: replace start_attempt with Ok(())
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        assert_eq!(delivery.status, WebhookDeliveryStatus::Pending);
        assert_eq!(delivery.attempts, 0);

        delivery.start_attempt().unwrap();
        assert_eq!(delivery.status, WebhookDeliveryStatus::Attempting);
        assert_eq!(delivery.attempts, 1);
    }

    #[test]
    fn test_webhook_delivery_start_attempt_increments_counter() {
        // Kill: replace += with -=, *= (attempts counter)
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        assert_eq!(delivery.attempts, 0);
        delivery.start_attempt().unwrap();
        assert_eq!(delivery.attempts, 1);
    }

    #[test]
    fn test_webhook_delivery_mark_delivered_changes_state() {
        // Kill: replace mark_delivered with Ok(())
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        delivery.start_attempt().unwrap();

        delivery.mark_delivered(200, None).unwrap();
        assert_eq!(delivery.status, WebhookDeliveryStatus::Delivered);
        assert!(delivery.delivered_at.is_some());
        assert_eq!(delivery.response_status, Some(200));
    }

    #[test]
    fn test_webhook_delivery_mark_for_retry_changes_state() {
        // Kill: replace mark_for_retry with Ok(())
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        delivery.start_attempt().unwrap();

        let retry_at = Utc::now() + chrono::Duration::minutes(5);
        delivery.mark_for_retry(Some(500), None, retry_at).unwrap();
        assert_eq!(delivery.status, WebhookDeliveryStatus::Retrying);
        assert_eq!(delivery.next_retry_at, Some(retry_at));
    }

    #[test]
    fn test_webhook_delivery_mark_failed_permanent_changes_state() {
        // Kill: replace mark_failed_permanent with Ok(())
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        delivery.start_attempt().unwrap();

        delivery.mark_failed_permanent(404, None).unwrap();
        assert_eq!(delivery.status, WebhookDeliveryStatus::Failed);
        assert_eq!(delivery.response_status, Some(404));
    }

    #[test]
    fn test_webhook_delivery_mark_failed_max_attempts_changes_state() {
        // Kill: replace mark_failed_max_attempts with Ok(())
        // MaxAttemptsExceeded event is only valid from Retrying state
        let mut delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        // Go through: Pending -> Attempting -> Retrying
        delivery.start_attempt().unwrap();
        let retry_at = Utc::now() + chrono::Duration::minutes(5);
        delivery.mark_for_retry(Some(500), None, retry_at).unwrap();
        assert_eq!(delivery.status, WebhookDeliveryStatus::Retrying);

        // From Retrying, mark as failed due to max attempts
        delivery.mark_failed_max_attempts().unwrap();
        assert_eq!(delivery.status, WebhookDeliveryStatus::Failed);
        assert!(delivery.next_retry_at.is_none());
    }

    #[test]
    fn test_webhook_delivery_can_transition_true_false() {
        // Kill: replace can_transition with true and false
        let delivery =
            WebhookDelivery::new(Uuid::new_v4(), None, "job.completed".to_string(), json!({}));
        // Pending can attempt
        assert!(delivery.can_transition(&WebhookDeliveryEvent::Attempt));
        // Pending cannot be delivered directly
        assert!(!delivery.can_transition(&WebhookDeliveryEvent::Success));
    }

    // ========================================================================
    // Mutant-killing tests: Usage
    // ========================================================================

    #[test]
    fn test_usage_net_credits_calculation() {
        // Kill: replace net_credits with 0; replace - with +
        let owner = Urn::user(Uuid::new_v4());
        let mut usage = Usage::new(owner, "2025-01".to_string()).unwrap();
        usage.credits_used = 100;
        usage.credits_refunded = 30;
        assert_eq!(usage.net_credits(), 70); // 100 - 30

        // Verify it's subtraction, not addition
        usage.credits_used = 50;
        usage.credits_refunded = 20;
        assert_eq!(usage.net_credits(), 30); // 50 - 20, NOT 50 + 20 = 70
    }

    #[test]
    fn test_usage_validate_returns_err_on_invalid() {
        // Kill: replace validate with Ok(())
        let mut usage = Usage::new(Urn::user(Uuid::new_v4()), "2025-01".to_string()).unwrap();
        usage.period = "invalid".to_string();
        assert!(usage.validate().is_err());
    }

    #[test]
    fn test_usage_validate_period_len() {
        // Kill: replace != with == (period len)
        let mut usage = Usage::new(Urn::user(Uuid::new_v4()), "2025-01".to_string()).unwrap();
        // Correct length (7) should pass
        assert!(usage.validate().is_ok());

        // Wrong length should fail
        usage.period = "25-01".to_string(); // 5 chars
        assert!(usage.validate().is_err());

        usage.period = "2025-012".to_string(); // 8 chars
        assert!(usage.validate().is_err());
    }

    #[test]
    fn test_usage_validate_period_regex() {
        // Kill: delete ! (regex match)
        let mut usage = Usage::new(Urn::user(Uuid::new_v4()), "2025-01".to_string()).unwrap();
        // Valid period should pass
        assert!(usage.validate().is_ok());

        // Invalid period format (right length, wrong format) should fail
        usage.period = "abcd-xy".to_string(); // 7 chars but doesn't match
        assert!(usage.validate().is_err());
    }

    #[test]
    fn test_usage_validate_counts_or_conditions() {
        // Kill: replace || with && (x2)
        let owner = Urn::user(Uuid::new_v4());

        // Only renders_count negative -> should fail
        let mut usage1 = Usage::new(owner.clone(), "2025-01".to_string()).unwrap();
        usage1.renders_count = -1;
        usage1.credits_used = 0;
        usage1.api_calls = 0;
        assert!(usage1.validate().is_err());

        // Only credits_used negative -> should fail
        let mut usage2 = Usage::new(owner.clone(), "2025-01".to_string()).unwrap();
        usage2.renders_count = 0;
        usage2.credits_used = -1;
        usage2.api_calls = 0;
        assert!(usage2.validate().is_err());

        // Only api_calls negative -> should fail
        let mut usage3 = Usage::new(owner.clone(), "2025-01".to_string()).unwrap();
        usage3.renders_count = 0;
        usage3.credits_used = 0;
        usage3.api_calls = -1;
        assert!(usage3.validate().is_err());
    }

    #[test]
    fn test_usage_validate_counts_boundary() {
        // Kill: replace < with ==, >, <= for all three count checks
        let owner = Urn::user(Uuid::new_v4());

        // All counts at 0 should be valid
        let mut usage = Usage::new(owner.clone(), "2025-01".to_string()).unwrap();
        usage.renders_count = 0;
        usage.credits_used = 0;
        usage.api_calls = 0;
        assert!(usage.validate().is_ok());

        // All counts at 1 should be valid
        usage.renders_count = 1;
        usage.credits_used = 1;
        usage.api_calls = 1;
        assert!(usage.validate().is_ok());

        // Each count at -1 individually should be invalid
        usage.renders_count = -1;
        usage.credits_used = 1;
        usage.api_calls = 1;
        assert!(usage.validate().is_err());

        usage.renders_count = 1;
        usage.credits_used = -1;
        usage.api_calls = 1;
        assert!(usage.validate().is_err());

        usage.renders_count = 1;
        usage.credits_used = 1;
        usage.api_calls = -1;
        assert!(usage.validate().is_err());
    }

    // ========================================================================
    // Mutant-killing tests: SystemAsset
    // ========================================================================

    #[test]
    fn test_system_asset_validate_returns_err_on_invalid() {
        // Kill: replace validate with Ok(())
        let asset = SystemAsset {
            id: "INVALID_FORMAT".to_string(),
            category: SystemAssetCategory::Sfx,
            name: "test".to_string(),
            description: "desc".to_string(),
            duration_seconds: None,
            s3_key: "key".to_string(),
            content_type: "audio/wav".to_string(),
            size_bytes: 1024,
            tags: Json(vec![]),
            created_at: Utc::now(),
        };
        assert!(asset.validate().is_err());
    }

    #[test]
    fn test_system_asset_validate_id_regex() {
        // Kill: delete ! (regex match)
        // Valid ID should pass
        let valid_asset = SystemAsset {
            id: "asset_sfx_test".to_string(),
            category: SystemAssetCategory::Sfx,
            name: "test".to_string(),
            description: "desc".to_string(),
            duration_seconds: None,
            s3_key: "key".to_string(),
            content_type: "audio/wav".to_string(),
            size_bytes: 1024,
            tags: Json(vec![]),
            created_at: Utc::now(),
        };
        assert!(valid_asset.validate().is_ok());

        // Invalid ID should fail
        let invalid_asset = SystemAsset {
            id: "not-matching-format".to_string(),
            ..valid_asset.clone()
        };
        assert!(invalid_asset.validate().is_err());
    }

    #[test]
    fn test_system_asset_validate_description_len_boundary() {
        // Kill: replace > with ==, <, >= (description len)
        let base = SystemAsset {
            id: "asset_sfx_test".to_string(),
            category: SystemAssetCategory::Sfx,
            name: "test".to_string(),
            description: "a".repeat(500),
            duration_seconds: None,
            s3_key: "key".to_string(),
            content_type: "audio/wav".to_string(),
            size_bytes: 1024,
            tags: Json(vec![]),
            created_at: Utc::now(),
        };
        // 500 chars should be valid
        assert!(base.validate().is_ok());

        // 501 chars should be invalid
        let asset_501 = SystemAsset {
            description: "a".repeat(501),
            ..base.clone()
        };
        assert!(asset_501.validate().is_err());

        // 499 chars should be valid
        let asset_499 = SystemAsset {
            description: "a".repeat(499),
            ..base.clone()
        };
        assert!(asset_499.validate().is_ok());
    }
}
