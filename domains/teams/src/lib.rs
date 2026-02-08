//! Teams domain: users, teams, memberships, invitations, API keys

pub mod api;
pub mod domain;
pub mod repository;

// Re-export domain types at the crate root for convenience
pub use domain::entities::*;
pub use domain::state::{
    InvitationEvent, InvitationGuardContext, InvitationState, InvitationStateMachine, StateError,
};
// Re-export repository types
pub use repository::{
    count_active_jobs_for_team_tx, count_for_user_tx, count_members_for_team_tx,
    count_owned_teams_tx, count_owners_for_team_tx, count_pending_for_team_tx,
    create_invitation_tx, create_membership_tx, create_team_tx, delete_membership_tx,
    delete_team_tx, delete_user_tx, find_teams_where_sole_owner_tx,
    get_membership_by_team_and_user_tx, get_team_by_slug_tx, mark_invitation_accepted_tx,
    revoke_invitation_tx, update_role_tx, upgrade_user_tier_tx, ApiKeyRepository,
    InvitationRepository, MembershipRepository, MembershipWithUser, TeamRepository,
    TeamsRepositories, UserRepository,
};

// Re-export API types
pub use api::routes;
pub use api::TeamsState;

// Re-export auth types from framecast-auth for backward compatibility
pub use framecast_auth::{
    AnyAuth, ApiKeyUser, AuthBackend, AuthConfig, AuthContext, AuthError, AuthUser, CreatorUser,
    SupabaseClaims,
};
