//! Concrete authentication backend
//!
//! Wraps `PgPool` + `AuthConfig` and owns auth-specific SQL queries.
//! Uses runtime `sqlx::query_as` (not macros) consistent with the
//! existing CQRS cross-domain read pattern.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::AuthConfig;
use crate::context::AuthContext;
use crate::error::AuthError;
use crate::types::{AuthApiKey, AuthIdentity, AuthMembership, AuthRole, AuthTier};

/// Row type for API key lookup (includes key_hash for verification)
#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    user_id: Uuid,
    owner: String,
    name: String,
    key_prefix: String,
    key_hash: String,
    scopes: serde_json::Value,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Row type for membership lookup
#[derive(sqlx::FromRow)]
struct MembershipRow {
    team_id: Uuid,
    team_name: String,
    team_slug: String,
    role: AuthRole,
}

/// Concrete authentication backend.
///
/// Wraps a database pool and auth configuration. Provides methods
/// to look up users, memberships, and API keys for authentication.
///
/// Domain states expose this via `FromRef`:
/// ```ignore
/// impl FromRef<MyDomainState> for AuthBackend {
///     fn from_ref(state: &MyDomainState) -> Self {
///         state.auth.clone()
///     }
/// }
/// ```
#[derive(Clone)]
pub struct AuthBackend {
    pool: PgPool,
    config: AuthConfig,
}

impl AuthBackend {
    pub fn new(pool: PgPool, config: AuthConfig) -> Self {
        Self { pool, config }
    }

    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// Find user identity by ID (CQRS read model — lightweight subset of User)
    pub(crate) async fn find_user(&self, id: Uuid) -> Result<Option<AuthIdentity>, AuthError> {
        let user: Option<AuthIdentity> = sqlx::query_as(
            r#"
            SELECT id, email, name, avatar_url,
                   tier, created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user_id = %id, "Failed to load user");
            AuthError::UserLoadError
        })?;

        Ok(user)
    }

    /// Find memberships for a user (CQRS read model — team_id + name + slug + role)
    pub(crate) async fn find_memberships(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<AuthMembership>, AuthError> {
        let rows: Vec<MembershipRow> = sqlx::query_as(
            r#"
            SELECT t.id as team_id, t.name as team_name, t.slug as team_slug,
                   m.role
            FROM teams t
            INNER JOIN memberships m ON t.id = m.team_id
            WHERE m.user_id = $1
            ORDER BY t.name ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user_id = %user_id, "Failed to load memberships");
            AuthError::MembershipsLoadError
        })?;

        Ok(rows
            .into_iter()
            .map(|r| AuthMembership {
                team_id: r.team_id,
                team_name: r.team_name,
                team_slug: r.team_slug,
                role: r.role,
            })
            .collect())
    }

    /// Authenticate an API key by raw key string.
    ///
    /// Replicates the SHA-256 + salt constant-time comparison from teams'
    /// `ApiKey::verify_key()` — the raw key material logic only.
    pub(crate) async fn authenticate_api_key(
        &self,
        candidate_key: &str,
    ) -> Result<Option<AuthApiKey>, AuthError> {
        if !candidate_key.starts_with("sk_live_") {
            return Ok(None);
        }

        let rows: Vec<ApiKeyRow> = sqlx::query_as(
            r#"
            SELECT id, user_id, owner, name, key_prefix, key_hash,
                   scopes, last_used_at, expires_at, revoked_at, created_at
            FROM api_keys
            WHERE key_prefix = $1 AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind("sk_live_")
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to query API keys");
            AuthError::AuthenticationFailed
        })?;

        for row in rows {
            if verify_key_hash(candidate_key, &row.key_hash) {
                // Update last_used_at (best-effort — don't fail auth on touch error)
                if let Err(e) =
                    sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
                        .bind(row.id)
                        .execute(&self.pool)
                        .await
                {
                    tracing::warn!(error = %e, api_key_id = %row.id, "Failed to update api_key last_used_at");
                }

                let scopes: Vec<String> =
                    serde_json::from_value(row.scopes).unwrap_or_else(|e| {
                        tracing::warn!(error = %e, api_key_id = %row.id, "Failed to deserialize api_key scopes, defaulting to empty");
                        vec![]
                    });

                return Ok(Some(AuthApiKey {
                    id: row.id,
                    user_id: row.user_id,
                    owner: row.owner,
                    name: row.name,
                    key_prefix: row.key_prefix,
                    scopes,
                    last_used_at: row.last_used_at,
                    expires_at: row.expires_at,
                    revoked_at: row.revoked_at,
                    created_at: row.created_at,
                }));
            }
        }

        Ok(None)
    }

    /// Shared JWT authentication logic used by both `AuthUser` and `AnyAuth`.
    pub(crate) async fn authenticate_jwt(&self, token: &str) -> Result<AuthContext, AuthError> {
        let claims = crate::jwt::validate_jwt_token(token, &self.config)?;

        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidUserId)?;

        let user = self
            .find_user(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        let memberships = if user.tier == AuthTier::Creator {
            self.find_memberships(user_id).await?
        } else {
            vec![]
        };

        Ok(AuthContext::new(user, memberships, None))
    }

    /// Shared API key authentication logic used by both `ApiKeyUser` and `AnyAuth`.
    pub(crate) async fn authenticate_api_key_full(
        &self,
        api_key_str: &str,
    ) -> Result<AuthContext, AuthError> {
        let authenticated_key = self
            .authenticate_api_key(api_key_str)
            .await?
            .ok_or(AuthError::InvalidApiKey)?;

        let user = self
            .find_user(authenticated_key.user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        let memberships = if user.tier == AuthTier::Creator {
            self.find_memberships(authenticated_key.user_id).await?
        } else {
            vec![]
        };

        Ok(AuthContext::new(user, memberships, Some(authenticated_key)))
    }
}

/// Verify an API key against stored hash using constant-time comparison.
///
/// Replicates `ApiKey::verify_key()` from the teams domain.
fn verify_key_hash(candidate_key: &str, stored_hash: &str) -> bool {
    // Parse stored hash: salt:hash
    let parts: Vec<&str> = stored_hash.split(':').collect();
    if parts.len() != 2 {
        return false;
    }

    let salt = match hex::decode(parts[0]) {
        Ok(salt) => salt,
        Err(_) => return false,
    };

    let hash = match hex::decode(parts[1]) {
        Ok(hash) => hash,
        Err(_) => return false,
    };

    // Compute hash of candidate key with stored salt
    let mut hasher = Sha256::new();
    hasher.update(candidate_key.as_bytes());
    hasher.update(&salt);
    let candidate_hash = hasher.finalize();

    // Constant-time comparison to prevent timing attacks
    if hash.len() != candidate_hash.len() {
        return false;
    }

    let mut result = 0u8;
    for (a, b) in hash.iter().zip(candidate_hash.iter()) {
        result |= a ^ b;
    }
    result == 0
}
