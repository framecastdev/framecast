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
        target_user_id: uuid::Uuid,
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

        // Users cannot remove themselves if they're the only owner
        if target_user_id == ctx.user.id && target_role == MembershipRole::Owner {
            // This would need to be checked against database for owner count
            // For now, assume this check happens at the repository level
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
}
