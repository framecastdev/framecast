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
) -> std::result::Result<(), sqlx::Error> {
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
) -> std::result::Result<Membership, sqlx::Error> {
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
        membership.role.clone() as MembershipRole,
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
) -> std::result::Result<i64, sqlx::Error> {
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
) -> std::result::Result<i64, sqlx::Error> {
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
) -> std::result::Result<Option<Membership>, sqlx::Error> {
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
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(team_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

/// Create a team within an existing transaction.
///
/// Uses runtime `query_as` to avoid SQLX offline cache requirements.
pub async fn create_team_tx(
    transaction: &mut Transaction<'_, Postgres>,
    team: &Team,
) -> std::result::Result<Team, sqlx::Error> {
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
