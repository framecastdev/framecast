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

    /// List teams for user with roles
    pub async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<(Team, MembershipRole)>> {
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

        let mut teams = Vec::with_capacity(rows.len());
        for row in rows {
            let settings = serde_json::from_value(row.settings).map_err(|e| {
                framecast_common::Error::Internal(format!(
                    "Failed to deserialize team settings: {}",
                    e
                ))
            })?;
            let team = Team {
                id: row.id,
                name: row.name,
                slug: row.slug,
                credits: row.credits,
                ephemeral_storage_bytes: row.ephemeral_storage_bytes,
                settings: sqlx::types::Json(settings),
                created_at: row.created_at,
                updated_at: row.updated_at,
            };
            teams.push((team, row.role));
        }

        Ok(teams)
    }
}
