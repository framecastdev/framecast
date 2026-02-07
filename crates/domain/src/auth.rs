//! Authorization and permission checking for Framecast
//!
//! This module implements the authorization layer that enforces access control
//! based on user tiers, team memberships, URN ownership, and API key scopes.

use crate::entities::*;
use framecast_common::{Error, Result, Urn, UrnComponents};
use std::collections::HashSet;
use thiserror::Error;

/// Authorization-specific error types
#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Insufficient permissions: {0}")]
    InsufficientPermissions(String),

    #[error("Invalid user tier for operation: {0}")]
    InvalidTier(String),

    #[error("Invalid scope for operation: {0}")]
    InvalidScope(String),

    #[error("Resource ownership violation: {0}")]
    OwnershipViolation(String),
}

impl From<AuthError> for Error {
    fn from(err: AuthError) -> Self {
        Error::Authorization(err.to_string())
    }
}

/// Represents an authenticated user context
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user: User,
    pub memberships: Vec<(Team, MembershipRole)>,
    pub api_key: Option<ApiKey>,
}

impl AuthContext {
    /// Create new auth context for a user
    pub fn new(
        user: User,
        memberships: Vec<(Team, MembershipRole)>,
        api_key: Option<ApiKey>,
    ) -> Self {
        Self {
            user,
            memberships,
            api_key,
        }
    }

    /// Check if user has creator tier
    pub fn is_creator(&self) -> bool {
        self.user.tier == UserTier::Creator
    }

    /// Check if user is starter tier
    pub fn is_starter(&self) -> bool {
        self.user.tier == UserTier::Starter
    }

    /// Get membership role for a specific team
    pub fn get_team_role(&self, team_id: uuid::Uuid) -> Option<MembershipRole> {
        self.memberships
            .iter()
            .find(|(team, _)| team.id == team_id)
            .map(|(_, role)| role.clone())
    }

    /// Check if user can access a URN
    pub fn can_access_urn(&self, urn: &Urn) -> bool {
        match urn.parse() {
            Ok(UrnComponents::User { user_id }) => user_id == self.user.id,
            Ok(UrnComponents::Team { team_id }) => self.get_team_role(team_id).is_some(),
            Ok(UrnComponents::TeamUser { team_id, user_id }) => {
                user_id == self.user.id && self.get_team_role(team_id).is_some()
            }
            Ok(UrnComponents::System { .. }) => false,
            Err(_) => false,
        }
    }

    /// Check if user has required scope (if using API key)
    pub fn has_scope(&self, required_scope: &str) -> bool {
        if let Some(api_key) = &self.api_key {
            // Wildcard scope allows everything
            if api_key.scopes.0.contains(&"*".to_string()) {
                return true;
            }

            // Check for exact scope match
            api_key.scopes.0.contains(&required_scope.to_string())
        } else {
            // If no API key, assume full access for authenticated user
            true
        }
    }
}

/// Permission checker for various operations
#[derive(Debug)]
pub struct PermissionChecker;

impl PermissionChecker {
    /// Check if user can perform job operations
    pub fn can_access_job(ctx: &AuthContext, job: &Job) -> Result<()> {
        // Parse job owner URN
        let job_urn: Urn = job
            .owner
            .parse()
            .map_err(|_| AuthError::OwnershipViolation("Invalid job owner URN".to_string()))?;

        // Check if user can access the job's URN
        if !ctx.can_access_urn(&job_urn) {
            return Err(AuthError::AccessDenied(
                "Cannot access job owned by different entity".to_string(),
            )
            .into());
        }

        Ok(())
    }

    /// Check if user can cancel a job
    pub fn can_cancel_job(ctx: &AuthContext, job: &Job) -> Result<()> {
        // First check basic access
        Self::can_access_job(ctx, job)?;

        // Check scope if using API key
        if !ctx.has_scope("jobs:write") {
            return Err(AuthError::InvalidScope("jobs:write scope required".to_string()).into());
        }

        // Only allow canceling non-terminal jobs
        if job.status.is_terminal() {
            return Err(AuthError::AccessDenied("Cannot cancel completed job".to_string()).into());
        }

        Ok(())
    }

    /// Check if user can clone a job
    pub fn can_clone_job(ctx: &AuthContext, job: &Job) -> Result<()> {
        // Basic access check
        Self::can_access_job(ctx, job)?;

        // Check scope
        if !ctx.has_scope("jobs:write") {
            return Err(AuthError::InvalidScope("jobs:write scope required".to_string()).into());
        }

        Ok(())
    }

    /// Check if user can create a team (creator tier only)
    pub fn can_create_team(ctx: &AuthContext) -> Result<()> {
        if !ctx.is_creator() {
            return Err(AuthError::InvalidTier(
                "Only creator tier users can create teams".to_string(),
            )
            .into());
        }

        // Check scope if using API key
        if !ctx.has_scope("team:admin") {
            return Err(AuthError::InvalidScope("team:admin scope required".to_string()).into());
        }

        Ok(())
    }

    /// Check if user can access team
    pub fn can_access_team(ctx: &AuthContext, team_id: uuid::Uuid) -> Result<()> {
        if ctx.get_team_role(team_id).is_none() {
            return Err(AuthError::AccessDenied("Not a team member".to_string()).into());
        }

        // Check scope if using API key
        if !ctx.has_scope("team:read") {
            return Err(AuthError::InvalidScope("team:read scope required".to_string()).into());
        }

        Ok(())
    }

    /// Check if user can perform team admin operations
    pub fn can_admin_team(ctx: &AuthContext, team_id: uuid::Uuid) -> Result<()> {
        // Must be owner or admin
        match ctx.get_team_role(team_id) {
            Some(MembershipRole::Owner) | Some(MembershipRole::Admin) => {}
            _ => {
                return Err(AuthError::InsufficientPermissions(
                    "Owner or admin role required".to_string(),
                )
                .into())
            }
        }

        // Check scope
        if !ctx.has_scope("team:admin") {
            return Err(AuthError::InvalidScope("team:admin scope required".to_string()).into());
        }

        Ok(())
    }

    /// Check if user can invite members to team
    pub fn can_invite_members(ctx: &AuthContext, team_id: uuid::Uuid) -> Result<()> {
        Self::can_admin_team(ctx, team_id)
    }

    /// Check if user can remove a team member
    pub fn can_remove_member(
        ctx: &AuthContext,
        team_id: uuid::Uuid,
        target_role: MembershipRole,
    ) -> Result<()> {
        // Check basic admin permissions
        Self::can_admin_team(ctx, team_id)?;

        let user_role = ctx
            .get_team_role(team_id)
            .ok_or_else(|| AuthError::AccessDenied("Not a team member".to_string()))?;

        // Owners cannot be removed by admins
        if target_role == MembershipRole::Owner && user_role != MembershipRole::Owner {
            return Err(
                AuthError::InsufficientPermissions("Cannot remove team owner".to_string()).into(),
            );
        }

        Ok(())
    }

    /// Check if user can change member role
    pub fn can_change_member_role(
        ctx: &AuthContext,
        team_id: uuid::Uuid,
        new_role: MembershipRole,
    ) -> Result<()> {
        let user_role = ctx
            .get_team_role(team_id)
            .ok_or_else(|| AuthError::AccessDenied("Not a team member".to_string()))?;

        // Only owners can promote to owner
        if new_role == MembershipRole::Owner && user_role != MembershipRole::Owner {
            return Err(AuthError::InsufficientPermissions(
                "Only owners can promote to owner role".to_string(),
            )
            .into());
        }

        // Check admin permissions for other roles
        Self::can_admin_team(ctx, team_id)?;

        Ok(())
    }

    /// Check if user can create project
    pub fn can_create_project(ctx: &AuthContext, team_id: uuid::Uuid) -> Result<()> {
        // Must be member or higher
        match ctx.get_team_role(team_id) {
            Some(MembershipRole::Owner)
            | Some(MembershipRole::Admin)
            | Some(MembershipRole::Member) => {}
            _ => {
                return Err(AuthError::InsufficientPermissions(
                    "Team membership required".to_string(),
                )
                .into())
            }
        }

        // Check scope
        if !ctx.has_scope("projects:write") {
            return Err(
                AuthError::InvalidScope("projects:write scope required".to_string()).into(),
            );
        }

        Ok(())
    }

    /// Check if user can delete project
    pub fn can_delete_project(ctx: &AuthContext, team_id: uuid::Uuid) -> Result<()> {
        // Must be owner or admin
        match ctx.get_team_role(team_id) {
            Some(MembershipRole::Owner) | Some(MembershipRole::Admin) => {}
            _ => {
                return Err(AuthError::InsufficientPermissions(
                    "Owner or admin role required".to_string(),
                )
                .into())
            }
        }

        // Check scope
        if !ctx.has_scope("projects:write") {
            return Err(
                AuthError::InvalidScope("projects:write scope required".to_string()).into(),
            );
        }

        Ok(())
    }

    /// Check if user can manage webhooks
    pub fn can_manage_webhooks(ctx: &AuthContext, team_id: uuid::Uuid) -> Result<()> {
        // Must be owner or admin
        match ctx.get_team_role(team_id) {
            Some(MembershipRole::Owner) | Some(MembershipRole::Admin) => {}
            _ => {
                return Err(AuthError::InsufficientPermissions(
                    "Owner or admin role required".to_string(),
                )
                .into())
            }
        }

        // Check scope
        if !ctx.has_scope("webhooks:write") {
            return Err(
                AuthError::InvalidScope("webhooks:write scope required".to_string()).into(),
            );
        }

        Ok(())
    }

    /// Check if user can access asset
    pub fn can_access_asset(ctx: &AuthContext, asset: &AssetFile) -> Result<()> {
        let asset_urn: Urn = asset
            .owner
            .parse()
            .map_err(|_| AuthError::OwnershipViolation("Invalid asset owner URN".to_string()))?;

        if !ctx.can_access_urn(&asset_urn) {
            return Err(AuthError::AccessDenied(
                "Cannot access asset owned by different entity".to_string(),
            )
            .into());
        }

        // Check scope
        if !ctx.has_scope("assets:read") {
            return Err(AuthError::InvalidScope("assets:read scope required".to_string()).into());
        }

        Ok(())
    }

    /// Validate API key scopes for user tier
    pub fn validate_api_key_scopes_for_tier(tier: UserTier, scopes: &[String]) -> Result<()> {
        let allowed_scopes: HashSet<&str> = match tier {
            UserTier::Starter => [
                "generate",
                "jobs:read",
                "jobs:write",
                "assets:read",
                "assets:write",
            ]
            .iter()
            .cloned()
            .collect(),
            UserTier::Creator => {
                // Creators can use all scopes
                return Ok(());
            }
        };

        for scope in scopes {
            if scope == "*" {
                // Wildcard allowed for all tiers
                continue;
            }

            if !allowed_scopes.contains(scope.as_str()) {
                return Err(AuthError::InvalidScope(format!(
                    "Scope '{}' not allowed for {} tier",
                    scope,
                    match tier {
                        UserTier::Starter => "starter",
                        UserTier::Creator => "creator",
                    }
                ))
                .into());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_user(tier: UserTier) -> User {
        User {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: Some("Test User".to_string()),
            avatar_url: None,
            tier,
            credits: 100,
            ephemeral_storage_bytes: 0,
            upgraded_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_team() -> Team {
        Team {
            id: Uuid::new_v4(),
            name: "Test Team".to_string(),
            slug: "test-team".to_string(),
            credits: 500,
            ephemeral_storage_bytes: 0,
            settings: sqlx::types::Json(std::collections::HashMap::new()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_auth_context_creator_check() {
        let creator_user = create_test_user(UserTier::Creator);
        let starter_user = create_test_user(UserTier::Starter);

        let creator_ctx = AuthContext::new(creator_user, vec![], None);
        let starter_ctx = AuthContext::new(starter_user, vec![], None);

        assert!(creator_ctx.is_creator());
        assert!(!creator_ctx.is_starter());

        assert!(!starter_ctx.is_creator());
        assert!(starter_ctx.is_starter());
    }

    #[test]
    fn test_urn_access_control() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let user_id = user.id;
        let team_id = team.id;

        let memberships = vec![(team, MembershipRole::Owner)];
        let ctx = AuthContext::new(user, memberships, None);

        // Can access own user URN
        let user_urn = Urn::user(user_id);
        assert!(ctx.can_access_urn(&user_urn));

        // Can access team URN with membership
        let team_urn = Urn::team(team_id);
        assert!(ctx.can_access_urn(&team_urn));

        // Can access team-user URN with matching user and team
        let team_user_urn = Urn::team_user(team_id, user_id);
        assert!(ctx.can_access_urn(&team_user_urn));

        // Cannot access other user's URN
        let other_user_urn = Urn::user(Uuid::new_v4());
        assert!(!ctx.can_access_urn(&other_user_urn));
    }

    #[test]
    fn test_api_key_scope_validation() {
        // Starter can use basic scopes
        let starter_scopes = vec!["generate".to_string(), "jobs:read".to_string()];
        assert!(PermissionChecker::validate_api_key_scopes_for_tier(
            UserTier::Starter,
            &starter_scopes
        )
        .is_ok());

        // Starter cannot use team scopes
        let team_scopes = vec!["team:admin".to_string()];
        assert!(PermissionChecker::validate_api_key_scopes_for_tier(
            UserTier::Starter,
            &team_scopes
        )
        .is_err());

        // Creator can use any scope
        let all_scopes = vec![
            "generate".to_string(),
            "team:admin".to_string(),
            "webhooks:write".to_string(),
        ];
        assert!(PermissionChecker::validate_api_key_scopes_for_tier(
            UserTier::Creator,
            &all_scopes
        )
        .is_ok());
    }

    #[test]
    fn test_team_permission_checks() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        // Test with owner role
        let owner_memberships = vec![(team.clone(), MembershipRole::Owner)];
        let owner_ctx = AuthContext::new(user.clone(), owner_memberships, None);

        assert!(PermissionChecker::can_access_team(&owner_ctx, team_id).is_ok());
        assert!(PermissionChecker::can_admin_team(&owner_ctx, team_id).is_ok());
        assert!(PermissionChecker::can_invite_members(&owner_ctx, team_id).is_ok());

        // Test with viewer role
        let viewer_memberships = vec![(team, MembershipRole::Viewer)];
        let viewer_ctx = AuthContext::new(user, viewer_memberships, None);

        assert!(PermissionChecker::can_access_team(&viewer_ctx, team_id).is_ok());
        assert!(PermissionChecker::can_admin_team(&viewer_ctx, team_id).is_err());
        assert!(PermissionChecker::can_invite_members(&viewer_ctx, team_id).is_err());
    }

    #[test]
    fn test_viewer_cannot_invite() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let viewer_memberships = vec![(team, MembershipRole::Viewer)];
        let viewer_ctx = AuthContext::new(user, viewer_memberships, None);

        assert!(PermissionChecker::can_invite_members(&viewer_ctx, team_id).is_err());
    }

    #[test]
    fn test_member_cannot_invite() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let member_memberships = vec![(team, MembershipRole::Member)];
        let member_ctx = AuthContext::new(user, member_memberships, None);

        assert!(PermissionChecker::can_invite_members(&member_ctx, team_id).is_err());
    }

    #[test]
    fn test_member_cannot_admin_team() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let member_memberships = vec![(team, MembershipRole::Member)];
        let member_ctx = AuthContext::new(user, member_memberships, None);

        assert!(PermissionChecker::can_admin_team(&member_ctx, team_id).is_err());
    }

    #[test]
    fn test_admin_can_admin_team() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let admin_memberships = vec![(team, MembershipRole::Admin)];
        let admin_ctx = AuthContext::new(user, admin_memberships, None);

        assert!(PermissionChecker::can_admin_team(&admin_ctx, team_id).is_ok());
        assert!(PermissionChecker::can_invite_members(&admin_ctx, team_id).is_ok());
    }

    #[test]
    fn test_creator_tier_requirements() {
        let starter_user = create_test_user(UserTier::Starter);
        let creator_user = create_test_user(UserTier::Creator);

        let starter_ctx = AuthContext::new(starter_user, vec![], None);
        let creator_ctx = AuthContext::new(creator_user, vec![], None);

        // Starters cannot create teams
        assert!(PermissionChecker::can_create_team(&starter_ctx).is_err());

        // Creators can create teams
        assert!(PermissionChecker::can_create_team(&creator_ctx).is_ok());
    }

    // --- Helper functions for new tests ---

    fn create_test_api_key(user_id: Uuid, scopes: Vec<String>) -> ApiKey {
        ApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: Urn::user(user_id).to_string(),
            name: "test-key".to_string(),
            key_prefix: "fc_test".to_string(),
            key_hash: "hash".to_string(),
            scopes: sqlx::types::Json(scopes),
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        }
    }

    fn create_test_job(owner_urn: &Urn, triggered_by: Uuid) -> Job {
        Job {
            id: Uuid::new_v4(),
            owner: owner_urn.to_string(),
            triggered_by,
            project_id: None,
            status: JobStatus::Queued,
            spec_snapshot: sqlx::types::Json(serde_json::json!({})),
            options: sqlx::types::Json(serde_json::json!({})),
            progress: sqlx::types::Json(serde_json::json!({})),
            output: None,
            output_size_bytes: None,
            error: None,
            credits_charged: 1,
            failure_type: None,
            credits_refunded: 0,
            idempotency_key: None,
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_asset(owner_urn: &Urn, uploaded_by: Uuid) -> AssetFile {
        AssetFile {
            id: Uuid::new_v4(),
            owner: owner_urn.to_string(),
            uploaded_by,
            project_id: None,
            filename: "test.png".to_string(),
            s3_key: "assets/test.png".to_string(),
            content_type: "image/png".to_string(),
            size_bytes: 1024,
            status: AssetStatus::Ready,
            metadata: sqlx::types::Json(serde_json::json!({})),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // --- Mutant-killing tests ---

    // Kills: crates/domain/src/auth.rs:82:41 replace && with || in AuthContext::can_access_urn
    // Tests that TeamUser URN requires BOTH user_id match AND team membership.
    #[test]
    fn test_team_user_urn_requires_both_user_and_team() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let user_id = user.id;
        let team_id = team.id;
        let other_team_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], None);

        // User matches, team matches -> allowed
        let urn_both = Urn::team_user(team_id, user_id);
        assert!(ctx.can_access_urn(&urn_both));

        // User matches, team does NOT match -> denied (kills && -> || mutant)
        let urn_wrong_team = Urn::team_user(other_team_id, user_id);
        assert!(!ctx.can_access_urn(&urn_wrong_team));

        // User does NOT match, team matches -> denied (kills && -> || mutant)
        let urn_wrong_user = Urn::team_user(team_id, other_user_id);
        assert!(!ctx.can_access_urn(&urn_wrong_user));
    }

    // Kills: crates/domain/src/auth.rs:91:9 replace AuthContext::has_scope -> bool with true
    // Tests that has_scope returns false when API key lacks the scope.
    #[test]
    fn test_has_scope_returns_false_when_scope_missing() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        // Scope present -> true
        assert!(ctx.has_scope("jobs:read"));

        // Scope not present -> false (kills "replace with true" mutant)
        assert!(!ctx.has_scope("jobs:write"));
        assert!(!ctx.has_scope("team:admin"));
    }

    // Kills: crates/domain/src/auth.rs:114:9 replace can_access_job -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:120:12 delete ! in can_access_job
    // Tests that can_access_job denies access for a job owned by a different entity.
    #[test]
    fn test_can_access_job_denies_different_owner() {
        let user = create_test_user(UserTier::Creator);
        let other_user_id = Uuid::new_v4();
        let job_owner = Urn::user(other_user_id);
        let job = create_test_job(&job_owner, other_user_id);

        let ctx = AuthContext::new(user, vec![], None);

        // Job owned by different user -> denied
        let result = PermissionChecker::can_access_job(&ctx, &job);
        assert!(result.is_err());
    }

    // Kills: crates/domain/src/auth.rs:133:9 replace can_cancel_job -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:136:12 delete ! in can_cancel_job
    // Tests that can_cancel_job denies when scope is missing.
    #[test]
    fn test_can_cancel_job_denies_wrong_scope() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let job_owner = Urn::user(user_id);
        let job = create_test_job(&job_owner, user_id);
        // API key with only read scope, not jobs:write
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        let result = PermissionChecker::can_cancel_job(&ctx, &job);
        assert!(result.is_err());
    }

    // Kills: crates/domain/src/auth.rs:151:9 replace can_clone_job -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:154:12 delete ! in can_clone_job
    // Tests that can_clone_job denies when scope is missing.
    #[test]
    fn test_can_clone_job_denies_wrong_scope() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let job_owner = Urn::user(user_id);
        let job = create_test_job(&job_owner, user_id);
        // API key with only read scope
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        let result = PermissionChecker::can_clone_job(&ctx, &job);
        assert!(result.is_err());
    }

    // Kills: crates/domain/src/auth.rs:180:9 replace can_access_team -> Result<()> with Ok(())
    // Tests that can_access_team denies non-members.
    #[test]
    fn test_can_access_team_denies_non_member() {
        let user = create_test_user(UserTier::Creator);
        let other_team_id = Uuid::new_v4();

        let ctx = AuthContext::new(user, vec![], None);

        let result = PermissionChecker::can_access_team(&ctx, other_team_id);
        assert!(result.is_err());
    }

    // Kills: crates/domain/src/auth.rs:226:9 replace can_remove_member -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:233:49 replace && with || in can_remove_member
    // Kills: crates/domain/src/auth.rs:233:24 replace == with != in can_remove_member
    // Kills: crates/domain/src/auth.rs:233:62 replace != with == in can_remove_member
    // Tests that admin cannot remove an owner.
    #[test]
    fn test_can_remove_member_admin_cannot_remove_owner() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        // User is admin, target is owner
        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Admin)], None);

        let result = PermissionChecker::can_remove_member(&ctx, team_id, MembershipRole::Owner);
        assert!(result.is_err(), "Admin should not be able to remove owner");
    }

    // Additional test to kill && -> || and == -> != mutants on line 233
    // Tests that owner CAN remove another owner (target_role == Owner, user_role == Owner).
    #[test]
    fn test_can_remove_member_owner_can_remove_owner() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        // User is owner, target is also owner
        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], None);

        let result = PermissionChecker::can_remove_member(&ctx, team_id, MembershipRole::Owner);
        assert!(
            result.is_ok(),
            "Owner should be able to remove another owner"
        );
    }

    // Tests that admin CAN remove a non-owner member (target_role != Owner).
    #[test]
    fn test_can_remove_member_admin_can_remove_member() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Admin)], None);

        let result = PermissionChecker::can_remove_member(&ctx, team_id, MembershipRole::Member);
        assert!(result.is_ok(), "Admin should be able to remove a member");
    }

    // Kills: crates/domain/src/auth.rs:254:9 replace can_change_member_role -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:259:46 replace && with || in can_change_member_role
    // Kills: crates/domain/src/auth.rs:259:21 replace == with != in can_change_member_role
    // Kills: crates/domain/src/auth.rs:259:59 replace != with == in can_change_member_role
    // Tests that non-owner cannot promote to owner.
    #[test]
    fn test_can_change_member_role_admin_cannot_promote_to_owner() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Admin)], None);

        let result =
            PermissionChecker::can_change_member_role(&ctx, team_id, MembershipRole::Owner);
        assert!(
            result.is_err(),
            "Admin should not be able to promote to owner"
        );
    }

    // Test that owner CAN promote to owner (kills == -> != on new_role and user_role)
    #[test]
    fn test_can_change_member_role_owner_can_promote_to_owner() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], None);

        let result =
            PermissionChecker::can_change_member_role(&ctx, team_id, MembershipRole::Owner);
        assert!(result.is_ok(), "Owner should be able to promote to owner");
    }

    // Test that admin can change role to non-owner role (kills && -> || mutant)
    #[test]
    fn test_can_change_member_role_admin_can_set_member_role() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Admin)], None);

        let result =
            PermissionChecker::can_change_member_role(&ctx, team_id, MembershipRole::Member);
        assert!(result.is_ok(), "Admin should be able to set member role");
    }

    // Kills: crates/domain/src/auth.rs:275:9 replace can_create_project -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:276:13 delete match arm Some(Owner) | Some(Admin) | Some(Member)
    // Tests that viewer cannot create project.
    #[test]
    fn test_can_create_project_viewer_denied() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Viewer)], None);

        let result = PermissionChecker::can_create_project(&ctx, team_id);
        assert!(
            result.is_err(),
            "Viewer should not be able to create project"
        );
    }

    // Test that non-member cannot create project (kills "replace with Ok()" mutant)
    #[test]
    fn test_can_create_project_non_member_denied() {
        let user = create_test_user(UserTier::Creator);
        let other_team_id = Uuid::new_v4();

        let ctx = AuthContext::new(user, vec![], None);

        let result = PermissionChecker::can_create_project(&ctx, other_team_id);
        assert!(
            result.is_err(),
            "Non-member should not be able to create project"
        );
    }

    // Kills: crates/domain/src/auth.rs:288:12 delete ! in can_create_project
    // Tests that scope denial works for can_create_project.
    #[test]
    fn test_can_create_project_scope_denied() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let team = create_test_team();
        let team_id = team.id;
        // API key without projects:write scope
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Member)], Some(api_key));

        let result = PermissionChecker::can_create_project(&ctx, team_id);
        assert!(result.is_err(), "Missing projects:write scope should deny");
    }

    // Verify member CAN create project (positive case to contrast with denial)
    #[test]
    fn test_can_create_project_member_allowed() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Member)], None);

        let result = PermissionChecker::can_create_project(&ctx, team_id);
        assert!(result.is_ok(), "Member should be able to create project");
    }

    // Kills: crates/domain/src/auth.rs:300:9 replace can_delete_project -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:301:13 delete match arm Some(Owner) | Some(Admin)
    // Tests that member cannot delete project.
    #[test]
    fn test_can_delete_project_member_denied() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Member)], None);

        let result = PermissionChecker::can_delete_project(&ctx, team_id);
        assert!(
            result.is_err(),
            "Member should not be able to delete project"
        );
    }

    // Kills: crates/domain/src/auth.rs:311:12 delete ! in can_delete_project
    // Tests that scope denial works for can_delete_project.
    #[test]
    fn test_can_delete_project_scope_denied() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let team = create_test_team();
        let team_id = team.id;
        // API key without projects:write scope
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], Some(api_key));

        let result = PermissionChecker::can_delete_project(&ctx, team_id);
        assert!(
            result.is_err(),
            "Missing projects:write scope should deny deletion"
        );
    }

    // Verify owner CAN delete project (positive case)
    #[test]
    fn test_can_delete_project_owner_allowed() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], None);

        let result = PermissionChecker::can_delete_project(&ctx, team_id);
        assert!(result.is_ok(), "Owner should be able to delete project");
    }

    // Kills: crates/domain/src/auth.rs:323:9 replace can_manage_webhooks -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:324:13 delete match arm Some(Owner) | Some(Admin)
    // Tests that member cannot manage webhooks.
    #[test]
    fn test_can_manage_webhooks_member_denied() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Member)], None);

        let result = PermissionChecker::can_manage_webhooks(&ctx, team_id);
        assert!(
            result.is_err(),
            "Member should not be able to manage webhooks"
        );
    }

    // Kills: crates/domain/src/auth.rs:334:12 delete ! in can_manage_webhooks
    // Tests that scope denial works for can_manage_webhooks.
    #[test]
    fn test_can_manage_webhooks_scope_denied() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let team = create_test_team();
        let team_id = team.id;
        // API key without webhooks:write scope
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], Some(api_key));

        let result = PermissionChecker::can_manage_webhooks(&ctx, team_id);
        assert!(result.is_err(), "Missing webhooks:write scope should deny");
    }

    // Verify owner CAN manage webhooks (positive case)
    #[test]
    fn test_can_manage_webhooks_owner_allowed() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Owner)], None);

        let result = PermissionChecker::can_manage_webhooks(&ctx, team_id);
        assert!(result.is_ok(), "Owner should be able to manage webhooks");
    }

    // Kills: crates/domain/src/auth.rs:345:9 replace can_access_asset -> Result<()> with Ok(())
    // Kills: crates/domain/src/auth.rs:350:12 delete ! in can_access_asset
    // Tests that can_access_asset denies access for asset owned by different entity.
    #[test]
    fn test_can_access_asset_wrong_owner_denied() {
        let user = create_test_user(UserTier::Creator);
        let other_user_id = Uuid::new_v4();
        let asset_owner = Urn::user(other_user_id);
        let asset = create_test_asset(&asset_owner, other_user_id);

        let ctx = AuthContext::new(user, vec![], None);

        let result = PermissionChecker::can_access_asset(&ctx, &asset);
        assert!(
            result.is_err(),
            "Asset owned by different user should be denied"
        );
    }

    // Kills: crates/domain/src/auth.rs:358:12 delete ! in can_access_asset
    // Tests that scope denial works for can_access_asset.
    #[test]
    fn test_can_access_asset_scope_denied() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let asset_owner = Urn::user(user_id);
        let asset = create_test_asset(&asset_owner, user_id);
        // API key without assets:read scope
        let api_key = create_test_api_key(user_id, vec!["jobs:read".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        let result = PermissionChecker::can_access_asset(&ctx, &asset);
        assert!(result.is_err(), "Missing assets:read scope should deny");
    }

    // Verify user CAN access own asset (positive case)
    #[test]
    fn test_can_access_asset_owner_allowed() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let asset_owner = Urn::user(user_id);
        let asset = create_test_asset(&asset_owner, user_id);

        let ctx = AuthContext::new(user, vec![], None);

        let result = PermissionChecker::can_access_asset(&ctx, &asset);
        assert!(result.is_ok(), "Owner should be able to access own asset");
    }

    // Additional test: can_access_job with own user URN succeeds (positive case)
    #[test]
    fn test_can_access_job_own_job_succeeds() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let job_owner = Urn::user(user_id);
        let job = create_test_job(&job_owner, user_id);

        let ctx = AuthContext::new(user, vec![], None);

        let result = PermissionChecker::can_access_job(&ctx, &job);
        assert!(result.is_ok(), "User should access own job");
    }

    // Additional test: can_cancel_job succeeds with correct scope
    #[test]
    fn test_can_cancel_job_with_correct_scope_succeeds() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let job_owner = Urn::user(user_id);
        let job = create_test_job(&job_owner, user_id);
        let api_key = create_test_api_key(user_id, vec!["jobs:write".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        let result = PermissionChecker::can_cancel_job(&ctx, &job);
        assert!(
            result.is_ok(),
            "Cancel with jobs:write scope should succeed"
        );
    }

    // Additional test: can_clone_job succeeds with correct scope
    #[test]
    fn test_can_clone_job_with_correct_scope_succeeds() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let job_owner = Urn::user(user_id);
        let job = create_test_job(&job_owner, user_id);
        let api_key = create_test_api_key(user_id, vec!["jobs:write".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        let result = PermissionChecker::can_clone_job(&ctx, &job);
        assert!(result.is_ok(), "Clone with jobs:write scope should succeed");
    }

    // Test wildcard scope allows everything
    #[test]
    fn test_has_scope_wildcard_allows_all() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let api_key = create_test_api_key(user_id, vec!["*".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        assert!(ctx.has_scope("jobs:write"));
        assert!(ctx.has_scope("team:admin"));
        assert!(ctx.has_scope("anything"));
    }

    // Test no API key means full access (has_scope returns true)
    #[test]
    fn test_has_scope_no_api_key_allows_all() {
        let user = create_test_user(UserTier::Creator);
        let ctx = AuthContext::new(user, vec![], None);

        assert!(ctx.has_scope("jobs:write"));
        assert!(ctx.has_scope("team:admin"));
    }

    // Test can_cancel_job denies for terminal job (to ensure complete coverage)
    #[test]
    fn test_can_cancel_job_terminal_job_denied() {
        let user = create_test_user(UserTier::Creator);
        let user_id = user.id;
        let job_owner = Urn::user(user_id);
        let mut job = create_test_job(&job_owner, user_id);
        job.status = JobStatus::Completed;

        let ctx = AuthContext::new(user, vec![], None);

        let result = PermissionChecker::can_cancel_job(&ctx, &job);
        assert!(result.is_err(), "Cannot cancel completed job");
    }

    // Test can_remove_member denies for non-admin (kills "replace with Ok()" mutant)
    #[test]
    fn test_can_remove_member_viewer_denied() {
        let user = create_test_user(UserTier::Creator);
        let team = create_test_team();
        let team_id = team.id;

        let ctx = AuthContext::new(user, vec![(team, MembershipRole::Viewer)], None);

        let result = PermissionChecker::can_remove_member(&ctx, team_id, MembershipRole::Member);
        assert!(
            result.is_err(),
            "Viewer should not be able to remove members"
        );
    }

    // Test can_change_member_role denies for non-member
    #[test]
    fn test_can_change_member_role_non_member_denied() {
        let user = create_test_user(UserTier::Creator);
        let other_team_id = Uuid::new_v4();

        let ctx = AuthContext::new(user, vec![], None);

        let result =
            PermissionChecker::can_change_member_role(&ctx, other_team_id, MembershipRole::Member);
        assert!(
            result.is_err(),
            "Non-member should not be able to change roles"
        );
    }
}
