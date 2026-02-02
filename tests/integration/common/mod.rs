//! Common test utilities and fixtures for integration tests
//!
//! This module provides shared infrastructure for all integration tests including:
//! - Test database setup and cleanup
//! - Authentication helpers
//! - User and team fixtures
//! - HTTP client setup
//! - Mock email service for invitations
//! - Common assertions

use std::env;
use std::sync::Once;

use anyhow::Result;
use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderValue};
use chrono::Utc;
use framecast_api::middleware::{AppState, AuthConfig};
use framecast_db::repositories::Repositories;
use framecast_domain::entities::*;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

static INIT: Once = Once::new();

/// Test environment configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub database_url: String,
    pub jwt_secret: String,
    pub api_base_url: String,
}

impl TestConfig {
    pub fn from_env() -> Self {
        // Ensure test environment variables are loaded
        INIT.call_once(|| {
            dotenvy::from_filename(".env.test").ok();
            dotenvy::dotenv().ok();
        });

        Self {
            database_url: env::var("TEST_DATABASE_URL")
                .or_else(|_| env::var("DATABASE_URL"))
                .unwrap_or_else(|_| {
                    "postgresql://postgres:password@localhost:5432/framecast_test".to_string() // pragma: allowlist secret
                }),
            jwt_secret: env::var("TEST_JWT_SECRET")
                .unwrap_or_else(|_| "test_secret_key_for_testing_only".to_string()),
            api_base_url: env::var("TEST_API_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
        }
    }
}

/// Test application state with database connection
#[allow(dead_code)]
pub struct TestApp {
    pub state: AppState,
    pub config: TestConfig,
    pub pool: PgPool,
}

impl TestApp {
    /// Create a new test application with fresh database connection
    pub async fn new() -> Result<Self> {
        let config = TestConfig::from_env();

        let pool = sqlx::PgPool::connect(&config.database_url).await?;

        // Run migrations for test database
        sqlx::migrate!("../../migrations").run(&pool).await?;

        let repos = Repositories::new(pool.clone());

        let auth_config = AuthConfig {
            jwt_secret: config.jwt_secret.clone(),
            issuer: Some("framecast-test".to_string()),
            audience: Some("authenticated".to_string()),
        };

        let state = AppState { repos, auth_config };

        Ok(TestApp {
            state,
            config,
            pool,
        })
    }

    /// Start a database transaction for isolated testing
    pub async fn transaction(&self) -> Result<Transaction<'_, Postgres>> {
        Ok(self.pool.begin().await?)
    }

    /// Create test user in database
    pub async fn create_test_user(&self, tier: UserTier) -> Result<User> {
        let user_id = Uuid::new_v4();
        let email = format!("test_{}@framecast.test", user_id.simple());
        let name = Some(format!("Test User {}", &user_id.to_string()[0..8]));

        let mut user = User::new(user_id, email, name)?;
        let user_tier = tier.clone(); // Clone before move
        user.tier = user_tier;

        // Set upgraded_at for creator users (INV-U1)
        if tier == UserTier::Creator {
            user.upgraded_at = Some(Utc::now());
        }

        // Clone tier before move
        let user_tier = user.tier.clone();

        // Insert into database
        sqlx::query!(
            r#"
            INSERT INTO users (id, email, name, tier, credits, ephemeral_storage_bytes, upgraded_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            user.id,
            user.email,
            user.name,
            user_tier as UserTier,
            user.credits,
            user.ephemeral_storage_bytes,
            user.upgraded_at,
            user.created_at,
            user.updated_at
        ).execute(&self.pool).await?;

        Ok(user)
    }

    /// Create test team in database with owner membership
    pub async fn create_test_team(&self, owner_id: Uuid) -> Result<(Team, Membership)> {
        let team_id = Uuid::new_v4();
        let name = format!("Test Team {}", &team_id.to_string()[0..8]);
        let slug = format!(
            "test-team-{}",
            team_id.simple().to_string()[0..8].to_string()
        );

        let mut team = Team::new(name, Some(slug))?;
        team.id = team_id;

        // Insert team into database
        sqlx::query!(
            r#"
            INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            team.id,
            team.name,
            team.slug,
            team.credits,
            team.ephemeral_storage_bytes,
            serde_json::to_value(&team.settings.0)?,
            team.created_at,
            team.updated_at
        ).execute(&self.pool).await?;

        // Create owner membership
        let membership = Membership {
            id: Uuid::new_v4(),
            team_id: team.id,
            user_id: owner_id,
            role: MembershipRole::Owner,
            created_at: Utc::now(),
        };

        // Clone role before move
        let membership_role = membership.role.clone();

        sqlx::query!(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            membership.id,
            membership.team_id,
            membership.user_id,
            membership_role as MembershipRole,
            membership.created_at
        )
        .execute(&self.pool)
        .await?;

        Ok((team, membership))
    }

    /// Clean up test data (call in test teardown)
    pub async fn cleanup(&self) -> Result<()> {
        // Delete in order to respect foreign key constraints
        sqlx::query!("DELETE FROM memberships")
            .execute(&self.pool)
            .await?;
        sqlx::query!("DELETE FROM teams")
            .execute(&self.pool)
            .await?;
        sqlx::query!("DELETE FROM users")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// User fixture for testing different user tiers
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserFixture {
    pub user: User,
    pub jwt_token: String,
}

impl UserFixture {
    /// Create a starter user fixture
    pub async fn starter(app: &TestApp) -> Result<Self> {
        let user = app.create_test_user(UserTier::Starter).await?;
        let jwt_token = create_test_jwt(&user, &app.config.jwt_secret)?;

        Ok(Self { user, jwt_token })
    }

    /// Create a creator user fixture
    pub async fn creator(app: &TestApp) -> Result<Self> {
        let user = app.create_test_user(UserTier::Creator).await?;
        let jwt_token = create_test_jwt(&user, &app.config.jwt_secret)?;

        Ok(Self { user, jwt_token })
    }

    /// Create a creator user with team ownership
    pub async fn creator_with_team(app: &TestApp) -> Result<(Self, Team, Membership)> {
        let user = app.create_test_user(UserTier::Creator).await?;
        let jwt_token = create_test_jwt(&user, &app.config.jwt_secret)?;
        let (team, membership) = app.create_test_team(user.id).await?;

        Ok((Self { user, jwt_token }, team, membership))
    }

    /// Get authorization header for HTTP requests
    pub fn auth_header(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.jwt_token)).unwrap(),
        );
        headers
    }
}

/// Create a test JWT token for a user
pub fn create_test_jwt(user: &User, secret: &str) -> Result<String> {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        email: String,
        aud: String,
        role: String,
        framecast_tier: String,
        iat: u64,
        exp: u64,
    }

    let now = chrono::Utc::now().timestamp() as u64;

    let claims = TestClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        aud: "authenticated".to_string(),
        role: "authenticated".to_string(),
        framecast_tier: user.tier.to_string(),
        iat: now,
        exp: now + 3600, // 1 hour
    };

    let header = Header::new(Algorithm::HS256);
    let encoding_key = EncodingKey::from_secret(secret.as_ref());

    Ok(jsonwebtoken::encode(&header, &claims, &encoding_key)?)
}

/// Common test assertions
#[allow(dead_code)]
pub mod assertions {
    use super::*;

    /// Assert that a URN is valid and optionally of a specific type
    pub fn assert_valid_urn(urn: &str, expected_type: Option<&str>) {
        let parts: Vec<&str> = urn.split(':').collect();
        assert!(parts.len() >= 3, "Invalid URN format: {}", urn);
        assert_eq!(
            parts[0], "framecast",
            "URN must start with 'framecast': {}",
            urn
        );

        if let Some(expected) = expected_type {
            assert_eq!(
                parts[1], expected,
                "Expected URN type {}, got {}",
                expected, parts[1]
            );
        }
    }

    /// Assert that a user tier is valid
    pub fn assert_user_tier_valid(tier: &UserTier) {
        match tier {
            UserTier::Starter | UserTier::Creator => (),
        }
    }

    /// Assert that credits are non-negative (business invariant)
    pub fn assert_credits_non_negative(credits: i32) {
        assert!(credits >= 0, "Credits cannot be negative: {}", credits);
    }

    /// Assert that a timestamp is recent (within last minute)
    pub fn assert_timestamp_recent(timestamp: &chrono::DateTime<chrono::Utc>) {
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(*timestamp);
        assert!(
            diff.num_seconds() < 60,
            "Timestamp should be recent, but was {} seconds ago",
            diff.num_seconds()
        );
    }

    /// Assert that first timestamp is before second timestamp
    pub fn assert_timestamp_progression(
        first: &chrono::DateTime<chrono::Utc>,
        second: &chrono::DateTime<chrono::Utc>,
    ) {
        assert!(
            first <= second,
            "First timestamp {:?} should be before second timestamp {:?}",
            first,
            second
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_from_env() {
        let config = TestConfig::from_env();
        assert!(!config.database_url.is_empty());
        assert!(!config.jwt_secret.is_empty());
        assert!(!config.api_base_url.is_empty());
    }

    #[tokio::test]
    async fn test_jwt_creation() {
        let user = User::new(
            Uuid::new_v4(),
            "test@example.com".to_string(),
            Some("Test User".to_string()),
        )
        .unwrap();

        let token = create_test_jwt(&user, "test_secret").unwrap();
        assert!(!token.is_empty());
        assert!(token.contains('.'));
    }

    #[test]
    fn test_urn_assertions() {
        assertions::assert_valid_urn("framecast:user:usr_123", Some("user"));
        assertions::assert_valid_urn("framecast:team:tm_456", Some("team"));
    }

    #[test]
    fn test_credit_assertions() {
        assertions::assert_credits_non_negative(0);
        assertions::assert_credits_non_negative(100);

        // This should panic - credits cannot be negative
        // assertions::assert_credits_non_negative(-1);
    }
}

pub mod email_mock;
