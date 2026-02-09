//! Membership repository

use crate::domain::entities::{Membership, MembershipRole};
use framecast_common::Result;
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

    /// List all memberships for a team with user details
    pub async fn list_by_team(&self, team_id: Uuid) -> Result<Vec<MembershipWithUser>> {
        let memberships = sqlx::query_as!(
            MembershipWithUser,
            r#"
            SELECT m.id, m.team_id, m.user_id, m.role as "role: MembershipRole", m.created_at,
                   u.email as user_email, u.name as user_name, u.avatar_url as user_avatar_url
            FROM memberships m
            INNER JOIN users u ON m.user_id = u.id
            WHERE m.team_id = $1
            ORDER BY
                CASE m.role
                    WHEN 'owner' THEN 0
                    WHEN 'admin' THEN 1
                    WHEN 'member' THEN 2
                    WHEN 'viewer' THEN 3
                END ASC,
                u.name ASC NULLS LAST
            "#,
            team_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(memberships)
    }
}
