//! Repository implementations for Teams domain

pub mod api_keys;
pub mod invitations;
pub mod memberships;
pub mod teams;
pub mod transactions;
pub mod users;

use sqlx::{PgPool, Postgres, Transaction};

pub use api_keys::ApiKeyRepository;
pub use invitations::InvitationRepository;
pub use memberships::{MembershipRepository, MembershipWithUser};
pub use teams::TeamRepository;
pub use transactions::{
    count_active_jobs_for_team_tx, count_for_user_tx, count_members_for_team_tx,
    count_owned_teams_tx, count_owners_for_team_tx, count_pending_for_team_tx,
    create_invitation_tx, create_membership_tx, create_team_tx, delete_membership_tx,
    delete_team_tx, delete_user_tx, find_teams_where_sole_owner_tx,
    get_membership_by_team_and_user_tx, get_team_by_slug_tx, mark_invitation_accepted_tx,
    revoke_invitation_tx, update_role_tx, upgrade_user_tier_tx,
};
pub use users::UserRepository;

/// Combined repository access for the Teams domain
#[derive(Clone)]
pub struct TeamsRepositories {
    pool: PgPool,
    pub users: UserRepository,
    pub teams: TeamRepository,
    pub memberships: MembershipRepository,
    pub invitations: InvitationRepository,
    pub api_keys: ApiKeyRepository,
}

impl TeamsRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            users: UserRepository::new(pool.clone()),
            teams: TeamRepository::new(pool.clone()),
            memberships: MembershipRepository::new(pool.clone()),
            invitations: InvitationRepository::new(pool.clone()),
            api_keys: ApiKeyRepository::new(pool.clone()),
            pool,
        }
    }

    /// Begin a new database transaction.
    pub async fn begin(&self) -> std::result::Result<Transaction<'static, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }
}
