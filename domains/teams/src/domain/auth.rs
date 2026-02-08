//! Authorization and permission checking for Framecast
//!
//! This module implements the authorization layer that enforces access control
//! based on user tiers, team memberships, URN ownership, and API key scopes.

use crate::domain::entities::*;
use framecast_common::{Urn, UrnComponents};

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
            .map(|(_, role)| *role)
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

    // Kills: domains/teams/src/domain/auth.rs replace AuthContext::has_scope -> bool with true
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
}
