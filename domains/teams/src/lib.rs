//! Teams domain: users, teams, memberships, invitations, API keys

pub mod api;
pub mod domain;
pub mod repository;

// Re-export domain types at the crate root for convenience
pub use domain::auth::AuthContext;
pub use domain::entities::*;
pub use domain::state::{
    InvitationEvent, InvitationGuardContext, InvitationState, InvitationStateMachine, StateError,
};
// Re-export repository types
pub use repository::{
    create_membership_tx, create_team_tx, mark_invitation_accepted_tx, upgrade_user_tier_tx,
    ApiKeyRepository, InvitationRepository, MembershipRepository, MembershipWithUser,
    TeamRepository, TeamsRepositories, UserRepository,
};

// Re-export API types
pub use api::routes;
pub use api::{ApiKeyUser, AuthConfig, AuthError, AuthUser, SupabaseClaims, TeamsState};
