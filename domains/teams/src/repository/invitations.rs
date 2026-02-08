//! Invitation repository

use crate::domain::entities::{Invitation, InvitationRole};
use crate::domain::state::InvitationState;
use framecast_common::{RepositoryError, Result};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct InvitationRepository {
    pool: PgPool,
}

impl InvitationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find invitation by ID
    pub async fn get_by_id(&self, invitation_id: Uuid) -> Result<Option<Invitation>> {
        let row = sqlx::query_as!(
            Invitation,
            r#"
            SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                   token, expires_at, accepted_at, declined_at, revoked_at, created_at
            FROM invitations
            WHERE id = $1
            "#,
            invitation_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Find invitation by team and email
    pub async fn get_by_team_and_email(
        &self,
        team_id: Uuid,
        email: &str,
    ) -> Result<Option<Invitation>> {
        let row = sqlx::query_as!(
            Invitation,
            r#"
            SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                   token, expires_at, accepted_at, declined_at, revoked_at, created_at
            FROM invitations
            WHERE team_id = $1 AND email = $2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            team_id,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Create a new invitation
    pub async fn create(&self, invitation: &Invitation) -> Result<Invitation> {
        let created_invitation = sqlx::query_as!(
            Invitation,
            r#"
            INSERT INTO invitations (id, team_id, invited_by, email, role, token, expires_at, accepted_at, declined_at, revoked_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, team_id, invited_by, email, role as "role: InvitationRole",
                      token, expires_at, accepted_at, declined_at, revoked_at, created_at
            "#,
            invitation.id,
            invitation.team_id,
            invitation.invited_by,
            invitation.email,
            invitation.role.clone() as InvitationRole,
            invitation.token,
            invitation.expires_at,
            invitation.accepted_at,
            invitation.declined_at,
            invitation.revoked_at,
            invitation.created_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(created_invitation)
    }

    /// Decline invitation (invitee-initiated)
    pub async fn decline(&self, invitation_id: Uuid) -> Result<()> {
        let result = sqlx::query!(
            r#"
            UPDATE invitations
            SET declined_at = NOW()
            WHERE id = $1
            "#,
            invitation_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound.into());
        }
        Ok(())
    }

    /// Revoke invitation (admin-initiated)
    pub async fn revoke(&self, invitation_id: Uuid) -> Result<()> {
        let result = sqlx::query!(
            r#"
            UPDATE invitations
            SET revoked_at = NOW()
            WHERE id = $1
            "#,
            invitation_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound.into());
        }
        Ok(())
    }

    /// Find all invitations for a team, optionally filtered by derived state
    pub async fn find_by_team(
        &self,
        team_id: Uuid,
        state_filter: Option<InvitationState>,
    ) -> Result<Vec<Invitation>> {
        let rows = match state_filter {
            Some(InvitationState::Pending) => {
                sqlx::query_as!(
                    Invitation,
                    r#"
                    SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                           token, expires_at, accepted_at, declined_at, revoked_at, created_at
                    FROM invitations
                    WHERE team_id = $1
                      AND accepted_at IS NULL
                      AND declined_at IS NULL
                      AND revoked_at IS NULL
                      AND expires_at > NOW()
                    ORDER BY created_at DESC
                    "#,
                    team_id
                )
                .fetch_all(&self.pool)
                .await?
            }
            Some(InvitationState::Accepted) => {
                sqlx::query_as!(
                    Invitation,
                    r#"
                    SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                           token, expires_at, accepted_at, declined_at, revoked_at, created_at
                    FROM invitations
                    WHERE team_id = $1
                      AND accepted_at IS NOT NULL
                    ORDER BY created_at DESC
                    "#,
                    team_id
                )
                .fetch_all(&self.pool)
                .await?
            }
            Some(InvitationState::Declined) => {
                sqlx::query_as!(
                    Invitation,
                    r#"
                    SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                           token, expires_at, accepted_at, declined_at, revoked_at, created_at
                    FROM invitations
                    WHERE team_id = $1
                      AND declined_at IS NOT NULL
                    ORDER BY created_at DESC
                    "#,
                    team_id
                )
                .fetch_all(&self.pool)
                .await?
            }
            Some(InvitationState::Expired) => {
                sqlx::query_as!(
                    Invitation,
                    r#"
                    SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                           token, expires_at, accepted_at, declined_at, revoked_at, created_at
                    FROM invitations
                    WHERE team_id = $1
                      AND accepted_at IS NULL
                      AND declined_at IS NULL
                      AND revoked_at IS NULL
                      AND expires_at <= NOW()
                    ORDER BY created_at DESC
                    "#,
                    team_id
                )
                .fetch_all(&self.pool)
                .await?
            }
            Some(InvitationState::Revoked) => {
                sqlx::query_as!(
                    Invitation,
                    r#"
                    SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                           token, expires_at, accepted_at, declined_at, revoked_at, created_at
                    FROM invitations
                    WHERE team_id = $1
                      AND revoked_at IS NOT NULL
                    ORDER BY created_at DESC
                    "#,
                    team_id
                )
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as!(
                    Invitation,
                    r#"
                    SELECT id, team_id, invited_by, email, role as "role: InvitationRole",
                           token, expires_at, accepted_at, declined_at, revoked_at, created_at
                    FROM invitations
                    WHERE team_id = $1
                    ORDER BY created_at DESC
                    "#,
                    team_id
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows)
    }

    /// Extend invitation expiration to 7 days from now
    pub async fn extend_expiration(&self, invitation_id: Uuid) -> Result<Invitation> {
        let updated = sqlx::query_as!(
            Invitation,
            r#"
            UPDATE invitations
            SET expires_at = NOW() + INTERVAL '7 days'
            WHERE id = $1
            RETURNING id, team_id, invited_by, email, role as "role: InvitationRole",
                      token, expires_at, accepted_at, declined_at, revoked_at, created_at
            "#,
            invitation_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(RepositoryError::NotFound)?;

        Ok(updated)
    }
}
