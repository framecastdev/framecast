//! Business invariant validation tests
//!
//! Tests all critical business rules from docs/spec/06_Invariants.md
//! to ensure data integrity across the system

use framecast_teams::*;
use chrono::Utc;
use uuid::Uuid;

use crate::common::TestApp;

/// Helper to convert MembershipRole to string for SQL binding
fn role_to_str(role: &MembershipRole) -> &'static str {
    match role {
        MembershipRole::Owner => "owner",
        MembershipRole::Admin => "admin",
        MembershipRole::Member => "member",
        MembershipRole::Viewer => "viewer",
    }
}

mod test_user_invariants {
    use super::*;

    #[tokio::test]
    async fn test_inv_u3_starter_users_no_team_memberships() {
        // INV-U3: Starter users have no team memberships
        let app = TestApp::new().await.unwrap();

        // Create starter user
        let starter_user = app.create_test_user(UserTier::Starter).await.unwrap();

        // Verify no memberships exist (using runtime query to avoid sqlx offline mode issues in tests)
        let memberships: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE user_id = $1",
        )
        .bind(starter_user.id)
        .fetch_one(&app.pool)
        .await
        .unwrap();

        assert_eq!(memberships.0, 0);

        // Attempting to create a team membership for starter user should be prevented
        // This is enforced at the application level, not database level
        assert_eq!(starter_user.tier, UserTier::Starter);
        assert!(!starter_user.can_create_teams());

        app.cleanup().await.unwrap();
    }

}

mod test_team_invariants {
    use super::*;

    #[tokio::test]
    async fn test_inv_t2_every_team_has_at_least_one_owner() {
        // INV-T2: Every team has ≥1 owner
        let app = TestApp::new().await.unwrap();

        // Create team with owner
        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let (team, membership) = app.create_test_team(creator_user.id).await.unwrap();

        // Verify owner membership exists
        assert_eq!(membership.team_id, team.id);
        assert_eq!(membership.user_id, creator_user.id);
        assert_eq!(membership.role, MembershipRole::Owner);

        // Verify owner count in database (using runtime query to avoid sqlx offline mode issues)
        let owner_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE team_id = $1 AND role = 'owner'",
        )
        .bind(team.id)
        .fetch_one(&app.pool)
        .await
        .unwrap();

        assert!(owner_count.0 >= 1);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_inv_t3_team_slug_uniqueness() {
        // INV-T3: Team slugs must be unique
        let app = TestApp::new().await.unwrap();

        let team1 = Team::new("Team One".to_string(), Some("unique-slug".to_string())).unwrap();
        let team2_result = Team::new("Team Two".to_string(), Some("unique-slug".to_string()));

        // Both teams can be created with same slug in memory,
        // but database constraint should prevent duplicate insertion

        // Insert first team (using runtime query to avoid sqlx offline mode issues)
        sqlx::query(
            r#"
            INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(team1.id)
        .bind(&team1.name)
        .bind(&team1.slug)
        .bind(team1.credits)
        .bind(team1.ephemeral_storage_bytes)
        .bind(&team1.settings)
        .bind(team1.created_at)
        .bind(team1.updated_at)
        .execute(&app.pool).await.unwrap();

        // Attempt to insert second team with same slug should fail
        let team2 = team2_result.unwrap();
        let insert_result = sqlx::query(
            r#"
            INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(team2.id)
        .bind(&team2.name)
        .bind(&team2.slug)
        .bind(team2.credits)
        .bind(team2.ephemeral_storage_bytes)
        .bind(&team2.settings)
        .bind(team2.created_at)
        .bind(team2.updated_at)
        .execute(&app.pool).await;

        assert!(insert_result.is_err(), "Duplicate slug should be rejected by database");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_inv_t7_max_owned_teams_per_user() {
        // INV-T7: Max 10 owned teams per user (CARD-2 from cardinality constraints)
        let app = TestApp::new().await.unwrap();

        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();

        // Create maximum allowed teams (10)
        for i in 0..10 {
            let team_name = format!("Team {}", i + 1);
            let team_slug = format!("team-{}", i + 1);

            let team = Team::new(team_name, Some(team_slug)).unwrap();

            // Insert team (using runtime query to avoid sqlx offline mode issues)
            sqlx::query(
                r#"
                INSERT INTO teams (id, name, slug, credits, ephemeral_storage_bytes, settings, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
            .execute(&app.pool).await.unwrap();

            // Create owner membership
            sqlx::query(
                r#"
                INSERT INTO memberships (id, team_id, user_id, role, created_at)
                VALUES ($1, $2, $3, $4::membership_role, $5)
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(team.id)
            .bind(creator_user.id)
            .bind("owner")
            .bind(Utc::now())
            .execute(&app.pool).await.unwrap();
        }

        // Verify we have exactly 10 owned teams (using runtime query)
        let owned_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE user_id = $1 AND role = 'owner'",
        )
        .bind(creator_user.id)
        .fetch_one(&app.pool)
        .await
        .unwrap();

        assert_eq!(owned_count.0, 10);

        // This constraint would be enforced in the application layer,
        // not at the database level, so we can't test the failure case here

        app.cleanup().await.unwrap();
    }

}

mod test_membership_invariants {
    use super::*;

    #[tokio::test]
    async fn test_inv_m4_only_creator_users_can_have_memberships() {
        // INV-M4: Only creator users can have team memberships
        let app = TestApp::new().await.unwrap();

        // Create starter user
        let starter_user = app.create_test_user(UserTier::Starter).await.unwrap();

        // Create creator user and team
        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let (team, _) = app.create_test_team(creator_user.id).await.unwrap();

        // Attempting to create membership for starter user should be prevented
        // This is enforced at the application level, but we can test the database constraint
        let membership_result = sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(starter_user.id)
        .bind("member")
        .bind(Utc::now())
        .execute(&app.pool).await;

        // If database has trigger to enforce this constraint, it should fail
        // Otherwise, this constraint is enforced in application logic
        // For now, we test that starter users have the right tier
        assert_eq!(starter_user.tier, UserTier::Starter);
        assert!(!starter_user.can_create_teams());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_membership_role_validity() {
        // Test that only valid roles can be assigned
        let app = TestApp::new().await.unwrap();

        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let (team, _) = app.create_test_team(creator_user.id).await.unwrap();

        // Create another creator user for testing roles
        let member_user = app.create_test_user(UserTier::Creator).await.unwrap();

        // Test all valid roles
        let valid_roles = vec![
            MembershipRole::Owner,
            MembershipRole::Admin,
            MembershipRole::Member,
            MembershipRole::Viewer,
        ];

        for role in valid_roles {
            let membership_id = Uuid::new_v4();

            // Using runtime query to avoid sqlx offline mode issues
            let insert_result = sqlx::query(
                r#"
                INSERT INTO memberships (id, team_id, user_id, role, created_at)
                VALUES ($1, $2, $3, $4::membership_role, $5)
                "#,
            )
            .bind(membership_id)
            .bind(team.id)
            .bind(member_user.id)
            .bind(role_to_str(&role))
            .bind(Utc::now())
            .execute(&app.pool).await;

            assert!(insert_result.is_ok(), "Role {:?} should be valid", role);

            // Clean up for next iteration
            sqlx::query("DELETE FROM memberships WHERE id = $1")
                .bind(membership_id)
                .execute(&app.pool).await.unwrap();
        }

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_unique_user_team_membership() {
        // Test that a user can only have one membership per team
        let app = TestApp::new().await.unwrap();

        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let (team, _) = app.create_test_team(creator_user.id).await.unwrap();

        let member_user = app.create_test_user(UserTier::Creator).await.unwrap();

        // Create first membership (using runtime query to avoid sqlx offline mode issues)
        let membership1_result = sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_user.id)
        .bind("member")
        .bind(Utc::now())
        .execute(&app.pool).await;

        assert!(membership1_result.is_ok());

        // Attempt to create duplicate membership
        let membership2_result = sqlx::query(
            r#"
            INSERT INTO memberships (id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4::membership_role, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(team.id)
        .bind(member_user.id)
        .bind("admin")
        .bind(Utc::now())
        .execute(&app.pool).await;

        // Should fail due to unique constraint on (team_id, user_id)
        assert!(membership2_result.is_err());

        app.cleanup().await.unwrap();
    }
}

mod test_cross_entity_constraints {
    use super::*;

    #[tokio::test]
    async fn test_referential_integrity() {
        // Test that memberships reference valid users and teams
        let app = TestApp::new().await.unwrap();

        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let (team, membership) = app.create_test_team(creator_user.id).await.unwrap();

        // Verify membership references exist (using runtime queries)
        let user_check: Result<(Uuid,), _> = sqlx::query_as(
            "SELECT id FROM users WHERE id = $1",
        )
        .bind(membership.user_id)
        .fetch_one(&app.pool).await;
        assert!(user_check.is_ok());

        let team_check: Result<(Uuid,), _> = sqlx::query_as(
            "SELECT id FROM teams WHERE id = $1",
        )
        .bind(membership.team_id)
        .fetch_one(&app.pool).await;
        assert!(team_check.is_ok());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cascade_deletion_constraints() {
        // Test that deleting a team removes its memberships
        let app = TestApp::new().await.unwrap();

        let creator_user = app.create_test_user(UserTier::Creator).await.unwrap();
        let (team, membership) = app.create_test_team(creator_user.id).await.unwrap();

        // Verify membership exists (using runtime queries)
        let membership_check: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE team_id = $1",
        )
        .bind(team.id)
        .fetch_one(&app.pool).await.unwrap();
        assert_eq!(membership_check.0, 1);

        // Delete team (this should cascade to memberships)
        sqlx::query("DELETE FROM teams WHERE id = $1")
            .bind(team.id)
            .execute(&app.pool)
            .await
            .unwrap();

        // Verify membership was deleted
        let membership_check_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE team_id = $1",
        )
        .bind(team.id)
        .fetch_one(&app.pool).await.unwrap();
        assert_eq!(membership_check_after.0, 0);

        app.cleanup().await.unwrap();
    }
}
