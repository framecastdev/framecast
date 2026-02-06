//! Repository implementations for Framecast
//!
//! This module provides database access layer using the repository pattern.
//! All repositories use sqlx with PostgreSQL and follow production-ready practices.

use framecast_common::{Error, Result};
use framecast_domain::entities::*;
use sqlx::{types::Json, PgPool, Postgres, Transaction};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Database-specific error types
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Record not found")]
    NotFound,

    #[error("Record already exists")]
    AlreadyExists,

    #[error("Database constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("Database connection error: {0}")]
    Connection(#[from] sqlx::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<RepositoryError> for Error {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound => Error::NotFound("Record not found".to_string()),
            RepositoryError::AlreadyExists => Error::Conflict("Record already exists".to_string()),
            RepositoryError::ConstraintViolation(msg) => Error::Validation(msg),
            RepositoryError::Connection(e) => Error::Database(e),
            RepositoryError::InvalidData(msg) => Error::Validation(msg),
        }
    }
}

// =============================================================================
// User Repository
// =============================================================================

#[derive(Clone)]
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find user by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, name, avatar_url,
                   tier as "tier: UserTier", credits,
                   ephemeral_storage_bytes, upgraded_at,
                   created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Find user by email
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, name, avatar_url,
                   tier as "tier: UserTier", credits,
                   ephemeral_storage_bytes, upgraded_at,
                   created_at, updated_at
            FROM users
            WHERE email = $1
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Create new user
    pub async fn create(&self, user: &User) -> Result<User> {
        let created = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (
                id, email, name, avatar_url, tier, credits,
                ephemeral_storage_bytes, upgraded_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user.id,
            user.email,
            user.name,
            user.avatar_url,
            user.tier.clone() as UserTier,
            user.credits,
            user.ephemeral_storage_bytes,
            user.upgraded_at,
            user.created_at,
            user.updated_at
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

    /// Update user credits atomically
    pub async fn update_credits(&self, user_id: Uuid, credits_delta: i32) -> Result<User> {
        let updated = sqlx::query_as!(
            User,
            r#"
            UPDATE users SET
                credits = credits + $2,
                updated_at = NOW()
            WHERE id = $1 AND credits + $2 >= 0
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user_id,
            credits_delta
        )
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or(
            RepositoryError::ConstraintViolation("Credits would become negative".to_string())
                .into(),
        )
    }

    /// Update user profile (name, avatar_url)
    pub async fn update_profile(
        &self,
        user_id: Uuid,
        name: Option<String>,
        avatar_url: Option<String>,
    ) -> Result<Option<User>> {
        let updated = sqlx::query_as!(
            User,
            r#"
            UPDATE users SET
                name = $2,
                avatar_url = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user_id,
            name,
            avatar_url
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(updated)
    }

    /// Upgrade user tier
    pub async fn upgrade_tier(&self, user_id: Uuid, new_tier: UserTier) -> Result<Option<User>> {
        let now = chrono::Utc::now();
        let updated = sqlx::query_as!(
            User,
            r#"
            UPDATE users SET
                tier = $2,
                upgraded_at = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, email, name, avatar_url,
                      tier as "tier: UserTier", credits,
                      ephemeral_storage_bytes, upgraded_at,
                      created_at, updated_at
            "#,
            user_id,
            new_tier as UserTier,
            now
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(updated)
    }
}

// =============================================================================
// Membership Repository
// =============================================================================

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

    /// Find membership by team and user
    pub async fn find(&self, team_id: Uuid, user_id: Uuid) -> Result<Option<Membership>> {
        let membership = sqlx::query_as!(
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

        Ok(membership)
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

// =============================================================================
// Team Repository (simplified)
// =============================================================================

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
            ORDER BY t.created_at ASC
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

// =============================================================================
// Invitation Repository
// =============================================================================

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

    /// Count pending invitations for a team
    pub async fn count_pending_for_team(&self, team_id: Uuid) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM invitations
            WHERE team_id = $1
              AND accepted_at IS NULL
              AND declined_at IS NULL
              AND revoked_at IS NULL
              AND expires_at > NOW()
            "#,
            team_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap_or(0))
    }

    /// Mark invitation as accepted
    pub async fn mark_accepted(&self, invitation_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE invitations
            SET accepted_at = NOW()
            WHERE id = $1
            "#,
            invitation_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Decline invitation (invitee-initiated)
    pub async fn decline(&self, invitation_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE invitations
            SET declined_at = NOW()
            WHERE id = $1
            "#,
            invitation_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Revoke invitation (admin-initiated)
    pub async fn revoke(&self, invitation_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE invitations
            SET revoked_at = NOW()
            WHERE id = $1
            "#,
            invitation_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find all invitations for a team
    pub async fn find_by_team(&self, team_id: Uuid) -> Result<Vec<Invitation>> {
        let rows = sqlx::query_as!(
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
        .await?;

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
        .fetch_one(&self.pool)
        .await?;

        Ok(updated)
    }
}

// =============================================================================
// Job Repository (Placeholder)
// =============================================================================

#[derive(Clone)]
pub struct JobRepository {
    #[allow(dead_code)] // Placeholder for future job functionality
    pool: PgPool,
}

impl JobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Count active jobs for a team (placeholder implementation)
    pub async fn count_active_jobs_for_team(&self, _team_id: Uuid) -> Result<i64> {
        // Placeholder implementation until Job entity is available
        // For now, return 0 to allow team deletion
        Ok(0)
    }
}

// =============================================================================
// API Key Repository (simplified)
// =============================================================================

#[derive(Clone)]
pub struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find API key by ID
    pub async fn find(&self, id: Uuid) -> Result<Option<ApiKey>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, owner, name, key_prefix, key_hash,
                   scopes, last_used_at, expires_at, revoked_at, created_at
            FROM api_keys
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let scopes: Vec<String> = serde_json::from_value(row.scopes)
                .map_err(|e| RepositoryError::InvalidData(format!("Invalid scopes JSON: {}", e)))?;

            let api_key = ApiKey {
                id: row.id,
                user_id: row.user_id,
                owner: row.owner,
                name: row.name,
                key_prefix: row.key_prefix,
                key_hash: row.key_hash,
                scopes: sqlx::types::Json(scopes),
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
                created_at: row.created_at,
            };
            Ok(Some(api_key))
        } else {
            Ok(None)
        }
    }

    /// Authenticate by API key
    pub async fn authenticate(&self, candidate_key: &str) -> Result<Option<ApiKey>> {
        // Extract key prefix
        if !candidate_key.starts_with("sk_live_") {
            return Ok(None);
        }

        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, owner, name, key_prefix, key_hash,
                   scopes, last_used_at, expires_at, revoked_at, created_at
            FROM api_keys
            WHERE key_prefix = $1 AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            "sk_live_"
        )
        .fetch_all(&self.pool)
        .await?;

        // Find the matching key using constant-time verification
        for row in rows {
            let scopes: Vec<String> = serde_json::from_value(row.scopes)
                .map_err(|e| RepositoryError::InvalidData(format!("Invalid scopes JSON: {}", e)))?;

            let api_key = ApiKey {
                id: row.id,
                user_id: row.user_id,
                owner: row.owner,
                name: row.name,
                key_prefix: row.key_prefix,
                key_hash: row.key_hash,
                scopes: sqlx::types::Json(scopes),
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
                created_at: row.created_at,
            };

            if api_key.verify_key(candidate_key) {
                // Update last_used_at
                sqlx::query!(
                    "UPDATE api_keys SET last_used_at = NOW() WHERE id = $1",
                    api_key.id
                )
                .execute(&self.pool)
                .await?;

                return Ok(Some(api_key));
            }
        }

        Ok(None)
    }

    /// Create new API key
    pub async fn create(&self, api_key: &ApiKey) -> Result<ApiKey> {
        sqlx::query!(
            r#"
            INSERT INTO api_keys (
                id, user_id, owner, name, key_prefix, key_hash,
                scopes, last_used_at, expires_at, revoked_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            api_key.id,
            api_key.user_id,
            api_key.owner,
            api_key.name,
            api_key.key_prefix,
            api_key.key_hash,
            serde_json::to_value(&api_key.scopes.0)?,
            api_key.last_used_at,
            api_key.expires_at,
            api_key.revoked_at,
            api_key.created_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                RepositoryError::AlreadyExists
            }
            _ => RepositoryError::from(e),
        })?;

        Ok(api_key.clone())
    }

    /// Revoke API key
    pub async fn revoke(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query!(
            "UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
            id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound.into());
        }

        Ok(())
    }
}

// =============================================================================
// Repository Collection
// =============================================================================

/// Combined repository access
#[derive(Clone)]
pub struct Repositories {
    pool: PgPool,
    pub users: UserRepository,
    pub teams: TeamRepository,
    pub memberships: MembershipRepository,
    pub invitations: InvitationRepository,
    pub jobs: JobRepository,
    pub api_keys: ApiKeyRepository,
}

impl Repositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            users: UserRepository::new(pool.clone()),
            teams: TeamRepository::new(pool.clone()),
            memberships: MembershipRepository::new(pool.clone()),
            invitations: InvitationRepository::new(pool.clone()),
            jobs: JobRepository::new(pool.clone()),
            api_keys: ApiKeyRepository::new(pool.clone()),
            pool,
        }
    }

    /// Begin a new database transaction.
    pub async fn begin(&self) -> std::result::Result<Transaction<'static, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }
}

// =============================================================================
// Transactional Free Functions (Zero2Prod pattern)
// =============================================================================

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn setup_test_db() -> PgPool {
        // This would be used for integration tests
        // For now, we'll skip actual database tests in this implementation
        PgPoolOptions::new()
            .max_connections(1)
            .connect("postgresql://localhost/framecast_test")
            .await
            .unwrap()
    }

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_user_repository_basic_operations() {
        let pool = setup_test_db().await;
        let _repo = UserRepository::new(pool);

        // Test would go here - create, find, update operations
        // This tests the repository against a real database
    }
}
