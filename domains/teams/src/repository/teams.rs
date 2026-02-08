//! Team repository

use crate::domain::entities::{MembershipRole, Team};
use framecast_common::Result;
use sqlx::types::Json;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct TeamRepository {
    pool: PgPool,
}

impl TeamRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find team by ID
    pub async fn get_by_id(&self, team_id: Uuid) -> Result<Option<Team>> {
        let row = sqlx::query_as!(
            Team,
            r#"
            SELECT id, name, slug, credits, ephemeral_storage_bytes,
                   settings as "settings: Json<HashMap<String, serde_json::Value>>",
                   created_at, updated_at
            FROM teams
            WHERE id = $1
            "#,
            team_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Find team by slug
    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<Team>> {
        let row = sqlx::query_as!(
            Team,
            r#"
            SELECT id, name, slug, credits, ephemeral_storage_bytes,
                   settings as "settings: Json<HashMap<String, serde_json::Value>>",
                   created_at, updated_at
            FROM teams
            WHERE slug = $1
            "#,
            slug
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Create a new team
    pub async fn create(&self, team: &Team) -> Result<Team> {
        let created_team = sqlx::query_as!(
            Team,
            r#"
            INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, name, slug, credits, ephemeral_storage_bytes,
                      settings as "settings: Json<HashMap<String, serde_json::Value>>",
                      created_at, updated_at
            "#,
            team.id,
            team.name,
            team.slug,
            team.credits,
            team.ephemeral_storage_bytes,
            &team.settings as &Json<HashMap<String, serde_json::Value>>,
            team.created_at,
            team.updated_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(created_team)
    }

    /// Update an existing team
    pub async fn update(&self, team: &Team) -> Result<Team> {
        let updated_team = sqlx::query_as!(
            Team,
            r#"
            UPDATE teams
            SET name = $2, settings = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING id, name, slug, credits, ephemeral_storage_bytes,
                      settings as "settings: Json<HashMap<String, serde_json::Value>>",
                      created_at, updated_at
            "#,
            team.id,
            team.name,
            &team.settings as &Json<HashMap<String, serde_json::Value>>
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(updated_team)
    }

    /// Delete a team
    pub async fn delete(&self, team_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM teams
            WHERE id = $1
            "#,
            team_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Count active (non-terminal) jobs for a team.
    ///
    /// CQRS read-side query: reads the jobs table directly so the teams domain
    /// doesn't need a compile-time dependency on the jobs crate.
    pub async fn count_active_jobs_for_team(&self, team_id: Uuid) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE owner LIKE 'framecast:team:' || $1::text || '%'
              AND status NOT IN ('completed', 'failed', 'canceled')
            "#,
        )
        .bind(team_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    /// Get teams for user with roles
    pub async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<(Team, MembershipRole)>> {
        let rows = sqlx::query!(
            r#"
            SELECT t.id, t.name, t.slug, t.credits, t.ephemeral_storage_bytes,
                   t.settings, t.created_at, t.updated_at,
                   m.role as "role: MembershipRole"
            FROM teams t
            INNER JOIN memberships m ON t.id = m.team_id
            WHERE m.user_id = $1
            ORDER BY t.name ASC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        let teams = rows
            .into_iter()
            .map(|row| {
                let team = Team {
                    id: row.id,
                    name: row.name,
                    slug: row.slug,
                    credits: row.credits,
                    ephemeral_storage_bytes: row.ephemeral_storage_bytes,
                    settings: sqlx::types::Json(
                        serde_json::from_value(row.settings)
                            .unwrap_or_else(|_| std::collections::HashMap::new()),
                    ),
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                };
                (team, row.role)
            })
            .collect();

        Ok(teams)
    }
}
