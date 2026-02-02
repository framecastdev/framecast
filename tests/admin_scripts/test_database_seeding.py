#!/usr/bin/env python3
"""
Database Seeding Script Tests - Rule 2 Compliance
Tests all seeding scenarios defined in TEST_STRATEGY.md

These tests validate:
- Happy path: Complete test data creation
- Edge cases: Existing data, partial failures
- Error conditions: Constraint violations, connectivity
- Invariants: All seeded data follows business rules
"""

import asyncio
import os
import subprocess
import sys
from typing import Any

import asyncpg
import pytest

# Add scripts directory to path
sys.path.insert(0, "/Users/thiago/Workscape/splice/scripts")
from seed import FramecastSeeder

# Test configuration
TEST_DATABASE_URL = os.getenv(
    "TEST_DATABASE_URL",
    "postgresql://postgres:dev-password-framecast@localhost:5432/framecast_test",
)


class SeedingTestFramework:
    """Framework for testing database seeding"""

    def __init__(self):
        self.conn = None
        self.test_db_name = None
        self.seeder = None

    async def setup_test_database(self):
        """Create isolated test database with migrations"""
        # Create test database
        base_url = TEST_DATABASE_URL.rsplit("/", 1)[0]
        self.test_db_name = (
            f"test_framecast_seed_{int(asyncio.get_event_loop().time())}"
        )
        test_db_url = f"{base_url}/postgres"

        conn = await asyncpg.connect(test_db_url)
        await conn.execute(f"CREATE DATABASE {self.test_db_name}")
        await conn.close()

        # Connect to test database
        self.test_db_url = f"{base_url}/{self.test_db_name}"
        self.conn = await asyncpg.connect(self.test_db_url)

        # Run migrations first
        await self.run_migrations()

        # Create seeder instance
        self.seeder = FramecastSeeder(self.test_db_url)

    async def teardown_test_database(self):
        """Clean up test database"""
        if self.seeder and self.seeder.conn:
            await self.seeder.disconnect()

        if self.conn:
            await self.conn.close()

        if self.test_db_name:
            base_url = TEST_DATABASE_URL.rsplit("/", 1)[0]
            test_db_url = f"{base_url}/postgres"
            conn = await asyncpg.connect(test_db_url)
            await conn.execute(f"DROP DATABASE IF EXISTS {self.test_db_name}")
            await conn.close()

    async def run_migrations(self):
        """Run database migrations on test database"""
        env = os.environ.copy()
        env["DATABASE_URL"] = self.test_db_url

        result = subprocess.run(
            ["sqlx", "migrate", "run", "--database-url", self.test_db_url],
            capture_output=True,
            text=True,
            env=env,
            cwd="/Users/thiago/Workscape/splice",
        )

        if result.returncode != 0:
            raise Exception(f"Migration failed: {result.stderr}")

    async def get_record_counts(self) -> dict[str, int]:
        """Get counts of all seeded record types"""
        tables = [
            "users",
            "teams",
            "memberships",
            "projects",
            "jobs",
            "api_keys",
            "system_assets",
        ]
        counts = {}

        for table in tables:
            count = await self.conn.fetchval(f"SELECT COUNT(*) FROM {table}")
            counts[table] = count

        return counts

    async def verify_user_data(self) -> dict[str, Any]:
        """Verify seeded user data integrity"""
        users = await self.conn.fetch("SELECT * FROM users ORDER BY email")

        verification = {
            "total_users": len(users),
            "creator_users": len([u for u in users if u["tier"] == "creator"]),
            "starter_users": len([u for u in users if u["tier"] == "starter"]),
            "users_with_credits": len([u for u in users if u["credits"] > 0]),
            "users_with_names": len([u for u in users if u["name"] is not None]),
        }

        return verification

    async def verify_team_data(self) -> dict[str, Any]:
        """Verify seeded team data integrity"""
        teams = await self.conn.fetch("SELECT * FROM teams")
        memberships = await self.conn.fetch("SELECT * FROM memberships")

        verification = {
            "total_teams": len(teams),
            "teams_with_credits": len([t for t in teams if t["credits"] > 0]),
            "total_memberships": len(memberships),
            "owner_memberships": len([m for m in memberships if m["role"] == "owner"]),
            "unique_team_slugs": len(set(t["slug"] for t in teams)),
        }

        return verification

    async def verify_business_rules(self) -> dict[str, bool]:
        """Verify all seeded data follows business rules"""
        checks = {}

        # Check: All teams have at least one owner
        teams_without_owners = await self.conn.fetchval("""
            SELECT COUNT(*) FROM teams t
            WHERE NOT EXISTS (
                SELECT 1 FROM memberships m
                WHERE m.team_id = t.id AND m.role = 'owner'
            )
        """)
        checks["teams_have_owners"] = teams_without_owners == 0

        # Check: Starter users have no memberships
        starter_memberships = await self.conn.fetchval("""
            SELECT COUNT(*) FROM memberships m
            JOIN users u ON m.user_id = u.id
            WHERE u.tier = 'starter'
        """)
        checks["starter_no_memberships"] = starter_memberships == 0

        # Check: All credits are non-negative
        negative_user_credits = await self.conn.fetchval(
            "SELECT COUNT(*) FROM users WHERE credits < 0"
        )
        negative_team_credits = await self.conn.fetchval(
            "SELECT COUNT(*) FROM teams WHERE credits < 0"
        )
        checks["non_negative_credits"] = (
            negative_user_credits == 0 and negative_team_credits == 0
        )

        # Check: All URN patterns are valid
        invalid_job_urns = await self.conn.fetchval("""
            SELECT COUNT(*) FROM jobs
            WHERE owner NOT SIMILAR TO 'framecast:(user|team):[a-zA-Z0-9_-]+'
        """)
        checks["valid_urns"] = invalid_job_urns == 0

        return checks


@pytest.fixture
async def seeding_framework():
    """Pytest fixture for seeding testing"""
    framework = SeedingTestFramework()
    await framework.setup_test_database()
    yield framework
    await framework.teardown_test_database()


# HAPPY PATH TESTS


@pytest.mark.asyncio
async def test_seed_happy_01_complete_test_data_creation(seeding_framework):
    """SEED-HAPPY-01: Complete test data creation"""
    framework = seeding_framework

    # Verify empty database initially
    initial_counts = await framework.get_record_counts()
    assert all(count == 0 for count in initial_counts.values()), (
        "Database should be empty before seeding"
    )

    # Run seeding
    await framework.seeder.seed_all(clear_existing=False)

    # Verify data was created
    final_counts = await framework.get_record_counts()

    assert final_counts["users"] >= 3, "Should create at least 3 test users"
    assert final_counts["teams"] >= 2, "Should create at least 2 test teams"
    assert final_counts["memberships"] >= 3, "Should create team memberships"
    assert final_counts["projects"] >= 2, "Should create test projects"
    assert final_counts["jobs"] >= 2, "Should create test jobs"
    assert final_counts["api_keys"] >= 2, "Should create test API keys"
    assert final_counts["system_assets"] >= 3, "Should create system assets"


@pytest.mark.asyncio
async def test_seed_happy_02_clear_and_reseed(seeding_framework):
    """SEED-HAPPY-02: Clear and re-seed functionality"""
    framework = seeding_framework

    # First seeding
    await framework.seeder.seed_all(clear_existing=False)
    first_counts = await framework.get_record_counts()

    # Second seeding with clear flag
    await framework.seeder.seed_all(clear_existing=True)
    second_counts = await framework.get_record_counts()

    # Should have same counts (cleared and re-created)
    assert second_counts["users"] == first_counts["users"], (
        "User count should be same after clear and re-seed"
    )
    assert second_counts["teams"] == first_counts["teams"], (
        "Team count should be same after clear and re-seed"
    )


# EDGE CASE TESTS


@pytest.mark.asyncio
async def test_seed_edge_01_existing_production_data(seeding_framework):
    """SEED-EDGE-01: Seed with existing production data"""
    framework = seeding_framework

    # Create some "production" data
    prod_user_id = await framework.conn.fetchval("""
        INSERT INTO users (email, name, tier)
        VALUES ('prod@company.com', 'Production User', 'creator')
        RETURNING id
    """)

    # Run seeding (should not affect production data)
    await framework.seeder.seed_all(clear_existing=False)

    # Verify production user still exists
    prod_user = await framework.conn.fetchrow(
        "SELECT * FROM users WHERE id = $1", prod_user_id
    )
    assert prod_user is not None, "Production user should still exist"
    assert prod_user["email"] == "prod@company.com", (
        "Production user data should be unchanged"
    )

    # Verify test data was created
    test_users = await framework.conn.fetch(
        "SELECT * FROM users WHERE email LIKE '%@test.framecast.dev'"
    )
    assert len(test_users) >= 3, "Test users should be created"


# ERROR CONDITION TESTS


@pytest.mark.asyncio
async def test_seed_error_01_constraint_violations(seeding_framework):
    """SEED-ERROR-01: Handle database constraint violations"""
    framework = seeding_framework

    # Create conflicting data that would cause constraint violation
    await framework.conn.execute("""
        INSERT INTO teams (name, slug)
        VALUES ('Existing Team', 'acme-studios-test')
    """)

    # Seeding should handle the conflict gracefully
    try:
        await framework.seeder.seed_all(clear_existing=False)
        # If it succeeds, verify it handled the conflict
        teams = await framework.conn.fetch(
            "SELECT * FROM teams WHERE slug LIKE 'acme-studios-test%'"
        )
        assert len(teams) >= 1, "Should handle slug conflicts"
    except Exception as e:
        # If it fails, should be a clear constraint error
        assert "constraint" in str(e).lower() or "unique" in str(e).lower(), (
            f"Should be clear constraint error: {e}"
        )


# INVARIANT TESTS


@pytest.mark.asyncio
async def test_seed_inv_01_all_data_follows_business_rules(seeding_framework):
    """SEED-INV-01: All seeded data follows business rules"""
    framework = seeding_framework

    # Run seeding
    await framework.seeder.seed_all(clear_existing=False)

    # Verify business rules
    rule_checks = await framework.verify_business_rules()

    for rule, passed in rule_checks.items():
        assert passed, f"Business rule violated: {rule}"


@pytest.mark.asyncio
async def test_seed_inv_02_user_tier_restrictions(seeding_framework):
    """Verify user tier restrictions are properly seeded"""
    framework = seeding_framework

    # Run seeding
    await framework.seeder.seed_all(clear_existing=False)

    # Verify user data integrity
    user_data = await framework.verify_user_data()

    assert user_data["creator_users"] >= 2, "Should have creator users"
    assert user_data["starter_users"] >= 1, "Should have starter users"
    assert user_data["users_with_credits"] >= 2, "Users should have credits"


@pytest.mark.asyncio
async def test_seed_inv_03_team_membership_integrity(seeding_framework):
    """Verify team membership integrity"""
    framework = seeding_framework

    # Run seeding
    await framework.seeder.seed_all(clear_existing=False)

    # Verify team data integrity
    team_data = await framework.verify_team_data()

    assert team_data["total_teams"] >= 2, "Should have teams"
    assert team_data["owner_memberships"] >= 2, "Each team should have owner"
    assert team_data["unique_team_slugs"] == team_data["total_teams"], (
        "All team slugs should be unique"
    )


# PERFORMANCE TESTS


@pytest.mark.asyncio
async def test_seed_perf_01_seeding_performance(seeding_framework):
    """PERF-SCRIPT-01: Seeding completes within reasonable time"""
    framework = seeding_framework

    import time

    start_time = time.time()

    await framework.seeder.seed_all(clear_existing=False)

    end_time = time.time()
    duration = end_time - start_time

    assert duration < 10, f"Seeding took {duration}s, should complete within 10s"


# INTEGRATION TESTS


@pytest.mark.asyncio
async def test_seed_integration_01_seed_then_query(seeding_framework):
    """Integration test: Seed data then perform complex queries"""
    framework = seeding_framework

    # Run seeding
    await framework.seeder.seed_all(clear_existing=False)

    # Test complex query that requires proper relationships
    team_stats = await framework.conn.fetchrow("""
        SELECT
            t.name,
            COUNT(DISTINCT m.user_id) as member_count,
            COUNT(DISTINCT p.id) as project_count,
            COUNT(DISTINCT j.id) as job_count,
            SUM(j.credits_charged) as total_credits_used
        FROM teams t
        LEFT JOIN memberships m ON t.id = m.team_id
        LEFT JOIN projects p ON t.id = p.team_id
        LEFT JOIN jobs j ON j.owner LIKE 'framecast:team:' || t.id::text
        WHERE t.slug = 'acme-studios-test'
        GROUP BY t.id, t.name
    """)

    assert team_stats is not None, "Should find seeded team"
    assert team_stats["member_count"] >= 1, "Team should have members"
    assert team_stats["project_count"] >= 1, "Team should have projects"


if __name__ == "__main__":
    # Run tests with pytest
    pytest.main([__file__, "-v", "--tb=short"])
