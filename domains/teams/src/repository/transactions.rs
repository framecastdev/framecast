//! Transactional free functions for Teams domain (Zero2Prod pattern)

use crate::domain::entities::{Membership, MembershipRole, UserTier};
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
