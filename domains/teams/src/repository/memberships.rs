//! Membership repository

use crate::domain::entities::{Membership, MembershipRole};
use framecast_common::{RepositoryError, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Membership with joined user details for list responses
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MembershipWithUser {
    pub id: Uuid,
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: MembershipRole,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub user_email: String,
    pub user_name: Option<String>,
    pub user_avatar_url: Option<String>,
}

#[derive(Clone)]
pub struct MembershipRepository {
    pool: PgPool,
}

impl MembershipRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Count how many teams a user owns
    pub async fn count_owned_teams(&self, user_id: Uuid) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM memberships
            WHERE user_id = $1 AND role = 'owner'
            "#,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// Get membership by team and user
    pub async fn get_by_team_and_user(
        &self,
        team_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Membership>> {
        let row = sqlx::query_as!(
            Membership,
            r#"
            SELECT id, team_id, user_id, role as "role: MembershipRole", created_at
            FROM memberships
            WHERE team_id = $1 AND user_id = $2
            "#,
            team_id,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Update a member's role
    pub async fn update_role(
        &self,
        team_id: Uuid,
        user_id: Uuid,
        new_role: MembershipRole,
    ) -> Result<Membership> {
        let updated_membership = sqlx::query_as!(
            Membership,
            r#"
            UPDATE memberships
            SET role = $3
            WHERE team_id = $1 AND user_id = $2
            RETURNING id, team_id, user_id, role as "role: MembershipRole", created_at
            "#,
            team_id,
            user_id,
            new_role as MembershipRole
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_membership)
    }

    /// Get all memberships for a team with user details
    pub async fn find_by_team(&self, team_id: Uuid) -> Result<Vec<MembershipWithUser>> {
        let memberships = sqlx::query_as!(
            MembershipWithUser,
            r#"
            SELECT m.id, m.team_id, m.user_id, m.role as "role: MembershipRole", m.created_at,
                   u.email as user_email, u.name as user_name, u.avatar_url as user_avatar_url
            FROM memberships m
            INNER JOIN users u ON m.user_id = u.id
            WHERE m.team_id = $1
            ORDER BY m.created_at ASC
            "#,
            team_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(memberships)
    }

    /// Create new membership
    pub async fn create(&self, membership: &Membership) -> Result<Membership> {
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
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                RepositoryError::AlreadyExists
            }
            _ => RepositoryError::from(e),
        })?;

        Ok(created)
    }

    /// Remove membership
    pub async fn delete(&self, team_id: Uuid, user_id: Uuid) -> Result<()> {
        let result = sqlx::query!(
            "DELETE FROM memberships WHERE team_id = $1 AND user_id = $2",
            team_id,
            user_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound.into());
        }

        Ok(())
    }

    /// Count all memberships for a user (INV-T8: max 50)
    pub async fn count_for_user(&self, user_id: Uuid) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM memberships
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// Count all memberships for a team
    pub async fn count_for_team(&self, team_id: Uuid) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM memberships
            WHERE team_id = $1
            "#,
            team_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// Count owners in team
    pub async fn count_owners(&self, team_id: Uuid) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM memberships
            WHERE team_id = $1 AND role = 'owner'
            "#,
            team_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }
}
