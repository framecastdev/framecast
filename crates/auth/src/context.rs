//! Authorization context for authenticated users

use crate::types::{AuthApiKey, AuthIdentity, AuthMembership, AuthRole, AuthTier};
use framecast_common::{Urn, UrnComponents};

/// Represents an authenticated user context
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user: AuthIdentity,
    pub memberships: Vec<AuthMembership>,
    pub api_key: Option<AuthApiKey>,
}

impl AuthContext {
    /// Create new auth context for a user
    pub fn new(
        user: AuthIdentity,
        memberships: Vec<AuthMembership>,
        api_key: Option<AuthApiKey>,
    ) -> Self {
        Self {
            user,
            memberships,
            api_key,
        }
    }

    /// Check if user has creator tier
    pub fn is_creator(&self) -> bool {
        self.user.tier == AuthTier::Creator
    }

    /// Check if user is starter tier
    pub fn is_starter(&self) -> bool {
        self.user.tier == AuthTier::Starter
    }

    /// Get membership role for a specific team
    pub fn get_team_role(&self, team_id: uuid::Uuid) -> Option<AuthRole> {
        self.memberships
            .iter()
            .find(|m| m.team_id == team_id)
            .map(|m| m.role)
    }

    /// Check if user can access a URN
    pub fn can_access_urn(&self, urn: &Urn) -> bool {
        match urn.parse() {
            Ok(UrnComponents::User { user_id }) => user_id == self.user.id,
            Ok(UrnComponents::Team { team_id }) => self.get_team_role(team_id).is_some(),
            Ok(UrnComponents::TeamUser { team_id, user_id }) => {
                user_id == self.user.id && self.get_team_role(team_id).is_some()
            }
            Ok(UrnComponents::Artifact { .. }) => false,
            Ok(UrnComponents::System { .. }) => false,
            Err(_) => false,
        }
    }

    /// Get all owner URNs accessible to this user: personal + each team membership
    pub fn accessible_owner_urns(&self) -> Vec<String> {
        let mut urns = vec![Urn::user(self.user.id).to_string()];
        for membership in &self.memberships {
            urns.push(Urn::team(membership.team_id).to_string());
        }
        urns
    }

    /// Check if user has required scope (if using API key)
    pub fn has_scope(&self, required_scope: &str) -> bool {
        if let Some(api_key) = &self.api_key {
            // Wildcard scope allows everything
            if api_key.scopes.contains(&"*".to_string()) {
                return true;
            }

            // Check for exact scope match
            api_key.scopes.contains(&required_scope.to_string())
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

    fn create_test_identity(tier: AuthTier) -> AuthIdentity {
        AuthIdentity {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: Some("Test User".to_string()),
            avatar_url: None,
            tier,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_membership(team_id: Uuid, role: AuthRole) -> AuthMembership {
        AuthMembership {
            team_id,
            team_name: "Test Team".to_string(),
            team_slug: "test-team".to_string(),
            role,
        }
    }

    #[test]
    fn test_auth_context_creator_check() {
        let creator_user = create_test_identity(AuthTier::Creator);
        let starter_user = create_test_identity(AuthTier::Starter);

        let creator_ctx = AuthContext::new(creator_user, vec![], None);
        let starter_ctx = AuthContext::new(starter_user, vec![], None);

        assert!(creator_ctx.is_creator());
        assert!(!creator_ctx.is_starter());

        assert!(!starter_ctx.is_creator());
        assert!(starter_ctx.is_starter());
    }

    #[test]
    fn test_urn_access_control() {
        let user = create_test_identity(AuthTier::Creator);
        let user_id = user.id;
        let team_id = Uuid::new_v4();

        let memberships = vec![create_test_membership(team_id, AuthRole::Owner)];
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

    fn create_test_api_key(user_id: Uuid, scopes: Vec<String>) -> AuthApiKey {
        AuthApiKey {
            id: Uuid::new_v4(),
            user_id,
            owner: Urn::user(user_id).to_string(),
            name: "test-key".to_string(),
            key_prefix: "fc_test".to_string(),
            scopes,
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_team_user_urn_requires_both_user_and_team() {
        let user = create_test_identity(AuthTier::Creator);
        let user_id = user.id;
        let team_id = Uuid::new_v4();
        let other_team_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();

        let ctx = AuthContext::new(
            user,
            vec![create_test_membership(team_id, AuthRole::Owner)],
            None,
        );

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

    // Kills: replace AuthContext::has_scope -> bool with true
    // Tests that has_scope returns false when API key lacks the scope.
    #[test]
    fn test_has_scope_returns_false_when_scope_missing() {
        let user = create_test_identity(AuthTier::Creator);
        let user_id = user.id;
        let api_key = create_test_api_key(user_id, vec!["generations:read".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        // Scope present -> true
        assert!(ctx.has_scope("generations:read"));

        // Scope not present -> false (kills "replace with true" mutant)
        assert!(!ctx.has_scope("generations:write"));
        assert!(!ctx.has_scope("team:admin"));
    }

    #[test]
    fn test_has_scope_wildcard_allows_all() {
        let user = create_test_identity(AuthTier::Creator);
        let user_id = user.id;
        let api_key = create_test_api_key(user_id, vec!["*".to_string()]);

        let ctx = AuthContext::new(user, vec![], Some(api_key));

        assert!(ctx.has_scope("generations:write"));
        assert!(ctx.has_scope("team:admin"));
        assert!(ctx.has_scope("anything"));
    }

    #[test]
    fn test_accessible_owner_urns_includes_user_and_teams() {
        let user = create_test_identity(AuthTier::Creator);
        let user_id = user.id;
        let team_a = Uuid::new_v4();
        let team_b = Uuid::new_v4();

        let ctx = AuthContext::new(
            user,
            vec![
                create_test_membership(team_a, AuthRole::Owner),
                create_test_membership(team_b, AuthRole::Member),
            ],
            None,
        );

        let urns = ctx.accessible_owner_urns();
        assert_eq!(urns.len(), 3);
        assert_eq!(urns[0], format!("framecast:user:{}", user_id));
        assert_eq!(urns[1], format!("framecast:team:{}", team_a));
        assert_eq!(urns[2], format!("framecast:team:{}", team_b));
    }

    #[test]
    fn test_accessible_owner_urns_no_teams() {
        let user = create_test_identity(AuthTier::Starter);
        let user_id = user.id;

        let ctx = AuthContext::new(user, vec![], None);

        let urns = ctx.accessible_owner_urns();
        assert_eq!(urns.len(), 1);
        assert_eq!(urns[0], format!("framecast:user:{}", user_id));
    }

    // Test no API key means full access (has_scope returns true)
    #[test]
    fn test_has_scope_no_api_key_allows_all() {
        let user = create_test_identity(AuthTier::Creator);
        let ctx = AuthContext::new(user, vec![], None);

        assert!(ctx.has_scope("generations:write"));
        assert!(ctx.has_scope("team:admin"));
    }
}
