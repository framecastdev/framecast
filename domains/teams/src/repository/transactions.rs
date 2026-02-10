//! Transactional free functions for Teams domain (Zero2Prod pattern)

use crate::domain::entities::{Membership, MembershipRole, Team, UserTier};
use framecast_common::RepositoryError;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Upgrade a user's tier within an existing transaction.
pub async fn upgrade_user_tier_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    new_tier: UserTier,
) -> std::result::Result<(), RepositoryError> {
    let now = chrono::Utc::now();
    sqlx::query!(
        r#"
        UPDATE users SET
            tier = $2,
            upgraded_at = $3,
            updated_at = NOW()
        WHERE id = $1
        "#,
        user_id,
        new_tier as UserTier,
        now
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Create a membership within an existing transaction.
pub async fn create_membership_tx(
    transaction: &mut Transaction<'_, Postgres>,
    membership: &Membership,
) -> std::result::Result<Membership, RepositoryError> {
    let created = sqlx::query_as!(
        Membership,
        r#"
        INSERT INTO memberships (id, team_id, user_id, role, created_at)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, team_id, user_id, role as "role: MembershipRole", created_at
        "#,
        membership.id,
        membership.team_id,
        membership.user_id,
        membership.role as MembershipRole,
        membership.created_at
    )
    .fetch_one(&mut **transaction)
    .await?;
    Ok(created)
}

/// Mark an invitation as accepted within an existing transaction.
///
/// Returns `RepositoryError::NotFound` if the invitation does not exist
/// or has already been accepted (accepted_at IS NOT NULL).
pub async fn mark_invitation_accepted_tx(
    transaction: &mut Transaction<'_, Postgres>,
    invitation_id: Uuid,
) -> std::result::Result<(), RepositoryError> {
    let result = sqlx::query!(
        r#"
        UPDATE invitations
        SET accepted_at = NOW()
        WHERE id = $1 AND accepted_at IS NULL
        "#,
        invitation_id
    )
    .execute(&mut **transaction)
    .await?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

/// Count all members for a team within an existing transaction.
///
/// Uses `FOR UPDATE` to lock all membership rows, preventing concurrent
/// modifications until the transaction completes. Call this early in the
/// transaction to serialise with other leave/remove operations (INV-T2).
///
/// PostgreSQL forbids `FOR UPDATE` with aggregate functions, so we fetch
/// the locked rows and count them in Rust.
pub async fn count_members_for_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
) -> std::result::Result<i64, RepositoryError> {
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM memberships WHERE team_id = $1 FOR UPDATE")
            .bind(team_id)
            .fetch_all(&mut **transaction)
            .await?;
    Ok(rows.len() as i64)
}

/// Count owners in a team within an existing transaction.
///
/// Assumes membership rows are already locked by a prior `FOR UPDATE` query
/// in the same transaction.
pub async fn count_owners_for_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
) -> std::result::Result<i64, RepositoryError> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND role = 'owner'")
            .bind(team_id)
            .fetch_one(&mut **transaction)
            .await?;
    Ok(row.0)
}

/// Get a membership by team and user within an existing transaction.
///
/// Uses runtime `query_as` to avoid SQLX offline cache requirements.
pub async fn get_membership_by_team_and_user_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
    user_id: Uuid,
) -> std::result::Result<Option<Membership>, RepositoryError> {
    let row: Option<Membership> = sqlx::query_as(
        r#"
        SELECT id, team_id, user_id, role, created_at
        FROM memberships
        WHERE team_id = $1 AND user_id = $2
        "#,
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(row)
}

/// Delete a membership within an existing transaction.
pub async fn delete_membership_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
    user_id: Uuid,
) -> std::result::Result<(), RepositoryError> {
    let result = sqlx::query("DELETE FROM memberships WHERE team_id = $1 AND user_id = $2")
        .bind(team_id)
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

/// Delete a team within an existing transaction.
pub async fn delete_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
) -> std::result::Result<(), RepositoryError> {
    let result = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(team_id)
        .execute(&mut **transaction)
        .await?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

/// Revoke an invitation within an existing transaction.
pub async fn revoke_invitation_tx(
    transaction: &mut Transaction<'_, Postgres>,
    invitation_id: Uuid,
) -> std::result::Result<(), RepositoryError> {
    let result = sqlx::query(
        "UPDATE invitations SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(invitation_id)
    .execute(&mut **transaction)
    .await?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

/// Create an invitation within an existing transaction.
pub async fn create_invitation_tx(
    transaction: &mut Transaction<'_, Postgres>,
    invitation: &crate::Invitation,
) -> std::result::Result<crate::Invitation, RepositoryError> {
    let created: crate::Invitation = sqlx::query_as(
        r#"
        INSERT INTO invitations (id, team_id, invited_by, email, role, token, expires_at, accepted_at, declined_at, revoked_at, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, team_id, invited_by, email, role, token, expires_at, accepted_at, declined_at, revoked_at, created_at
        "#,
    )
    .bind(invitation.id)
    .bind(invitation.team_id)
    .bind(invitation.invited_by)
    .bind(&invitation.email)
    .bind(invitation.role as crate::InvitationRole)
    .bind(&invitation.token)
    .bind(invitation.expires_at)
    .bind(invitation.accepted_at)
    .bind(invitation.declined_at)
    .bind(invitation.revoked_at)
    .bind(invitation.created_at)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(created)
}

/// Count all memberships for a user within an existing transaction.
///
/// Used for INV-U2 checks (creators must belong to ≥1 team) inside
/// transactions that already hold row locks, ensuring the count
/// participates in the same snapshot isolation.
pub async fn count_for_user_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> std::result::Result<i64, RepositoryError> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&mut **transaction)
        .await?;
    Ok(row.0)
}

/// Count pending invitations for a team within an existing transaction.
///
/// Used for CARD-4 checks (max 50 pending invitations) inside transactions
/// to prevent concurrent invitations from exceeding the limit.
pub async fn count_pending_for_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
) -> std::result::Result<i64, RepositoryError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM invitations
        WHERE team_id = $1
          AND accepted_at IS NULL
          AND declined_at IS NULL
          AND revoked_at IS NULL
          AND expires_at > NOW()
        "#,
    )
    .bind(team_id)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(row.0)
}

/// Update a membership's role within an existing transaction.
///
/// Uses runtime `query_as` to avoid SQLX offline cache requirements.
/// `MembershipRole` derives `sqlx::Type` so it encodes correctly via `.bind()`.
pub async fn update_role_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
    user_id: Uuid,
    new_role: MembershipRole,
) -> std::result::Result<Membership, RepositoryError> {
    let updated: Membership = sqlx::query_as(
        r#"
        UPDATE memberships
        SET role = $3
        WHERE team_id = $1 AND user_id = $2
        RETURNING id, team_id, user_id, role, created_at
        "#,
    )
    .bind(team_id)
    .bind(user_id)
    .bind(new_role)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(updated)
}

/// Count how many teams a user owns within an existing transaction.
///
/// Cross-team read used for INV-T7 (max owned teams) checks.
pub async fn count_owned_teams_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> std::result::Result<i64, RepositoryError> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memberships WHERE user_id = $1 AND role = 'owner'")
            .bind(user_id)
            .fetch_one(&mut **transaction)
            .await?;
    Ok(row.0)
}

/// Count active (non-terminal) jobs for a team within an existing transaction.
///
/// CQRS read-side query: reads the jobs table directly.
pub async fn count_active_jobs_for_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
) -> std::result::Result<i64, RepositoryError> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM jobs
        WHERE owner = 'framecast:team:' || $1::text
          AND status NOT IN ('completed', 'failed', 'canceled')
        "#,
    )
    .bind(team_id)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(row.0)
}

/// Get a team by slug within an existing transaction.
///
/// Uses `FOR UPDATE` to lock the row (if it exists), preventing concurrent
/// team creation with the same slug from racing past the uniqueness check.
pub async fn get_team_by_slug_tx(
    transaction: &mut Transaction<'_, Postgres>,
    slug: &str,
) -> std::result::Result<Option<Team>, RepositoryError> {
    let row: Option<Team> = sqlx::query_as(
        r#"
        SELECT id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at
        FROM teams
        WHERE slug = $1
        FOR UPDATE
        "#,
    )
    .bind(slug)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(row)
}

/// Find teams where user is the sole owner within an existing transaction.
///
/// Used for INV-T2 pre-checks (e.g. account deletion) inside a transaction
/// to prevent races between the check and subsequent mutations.
pub async fn find_teams_where_sole_owner_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> std::result::Result<Vec<Uuid>, RepositoryError> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT m.team_id
        FROM memberships m
        WHERE m.user_id = $1 AND m.role = 'owner'
          AND (
            SELECT COUNT(*)
            FROM memberships m2
            WHERE m2.team_id = m.team_id AND m2.role = 'owner'
          ) = 1
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut **transaction)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Delete a user within an existing transaction.
pub async fn delete_user_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> std::result::Result<(), RepositoryError> {
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

/// Create a team within an existing transaction.
///
/// Uses runtime `query_as` to avoid SQLX offline cache requirements.
pub async fn create_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team: &Team,
) -> std::result::Result<Team, RepositoryError> {
    let created: Team = sqlx::query_as(
        r#"
        INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at
        "#,
    )
    .bind(team.id)
    .bind(&team.name)
    .bind(&team.slug)
    .bind(team.credits)
    .bind(team.ephemeral_storage_bytes)
    .bind(&team.settings)
    .bind(team.created_at)
    .bind(team.updated_at)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(created)
}
