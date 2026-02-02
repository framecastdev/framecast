#!/usr/bin/env python3
"""
Database Migration Tests - Rule 2 Compliance
Tests all migration scenarios defined in TEST_STRATEGY.md

These tests validate:
- Happy path: Clean migrations from scratch
- Edge cases: Existing data, partial failures, concurrency
- Error conditions: Invalid SQL, connection issues, permissions
- Invariants: Business rules enforcement
"""

import asyncio
import os
import subprocess
import time
from typing import Any

import asyncpg
import pytest

# Test configuration
TEST_DATABASE_URL = os.getenv(
    "TEST_DATABASE_URL",
    "postgresql://postgres:dev-password-framecast@localhost:5432/framecast_test",  # pragma: allowlist secret
)


class MigrationTestFramework:
    """Framework for testing database migrations"""

    def __init__(self):
        self.conn = None
        self.test_db_name = None

    async def setup_test_database(self):
        """Create isolated test database for each test"""
        # Connect to postgres database to create test database
        base_url = TEST_DATABASE_URL.rsplit("/", 1)[0]
        self.test_db_name = f"test_framecast_{int(time.time())}"
        test_db_url = f"{base_url}/postgres"

        conn = await asyncpg.connect(test_db_url)
        await conn.execute(f"CREATE DATABASE {self.test_db_name}")
        await conn.close()

        # Connect to the test database
        self.test_db_url = f"{base_url}/{self.test_db_name}"
        self.conn = await asyncpg.connect(self.test_db_url)

    async def teardown_test_database(self):
        """Clean up test database"""
        if self.conn:
            await self.conn.close()

        if self.test_db_name:
            base_url = TEST_DATABASE_URL.rsplit("/", 1)[0]
            test_db_url = f"{base_url}/postgres"
            conn = await asyncpg.connect(test_db_url)
            await conn.execute(f"DROP DATABASE IF EXISTS {self.test_db_name}")
            await conn.close()

    async def run_migrations(self) -> dict[str, Any]:
        """Run migrations and return result"""
        try:
            # Use sqlx to run migrations
            env = os.environ.copy()
            env["DATABASE_URL"] = self.test_db_url

            result = subprocess.run(
                ["sqlx", "migrate", "run", "--database-url", self.test_db_url],
                capture_output=True,
                text=True,
                env=env,
                cwd="/Users/thiago/Workscape/splice",
            )

            return {
                "success": result.returncode == 0,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "return_code": result.returncode,
            }
        except Exception as e:
            return {"success": False, "error": str(e), "return_code": -1}

    async def get_migration_status(self) -> list[dict]:
        """Get current migration status"""
        try:
            migrations = await self.conn.fetch(
                "SELECT version, description, success FROM _sqlx_migrations ORDER BY version"
            )
            return [dict(m) for m in migrations]
        except Exception:
            return []

    async def get_table_count(self) -> int:
        """Get number of tables in database"""
        count = await self.conn.fetchval("""
            SELECT COUNT(*) FROM information_schema.tables
            WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
        """)
        return count

    async def check_table_exists(self, table_name: str) -> bool:
        """Check if table exists"""
        count = await self.conn.fetchval(
            """
            SELECT COUNT(*) FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = $1
        """,
            table_name,
        )
        return count > 0

    async def check_constraint_exists(self, constraint_name: str) -> bool:
        """Check if constraint exists"""
        count = await self.conn.fetchval(
            """
            SELECT COUNT(*) FROM information_schema.table_constraints
            WHERE constraint_name = $1
        """,
            constraint_name,
        )
        return count > 0

    async def test_invariant_violation(self, query: str, params: list = None) -> bool:
        """Test if query violates invariants (should fail)"""
        try:
            if params:
                await self.conn.execute(query, *params)
            else:
                await self.conn.execute(query)
            return False  # Should have failed
        except Exception:
            return True  # Expected failure


@pytest.fixture
async def migration_framework():
    """Pytest fixture for migration testing"""
    framework = MigrationTestFramework()
    await framework.setup_test_database()
    yield framework
    await framework.teardown_test_database()


# HAPPY PATH TESTS


@pytest.mark.asyncio
async def test_happy_01_clean_migration_from_scratch(migration_framework):
    """HAPPY-01: Clean database migration from scratch"""
    framework = migration_framework

    # Verify empty database
    initial_tables = await framework.get_table_count()
    assert initial_tables == 0, "Database should be empty initially"

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], f"Migrations failed: {result.get('stderr', '')}"

    # Verify all expected tables created
    final_tables = await framework.get_table_count()
    assert final_tables == 14, f"Expected 14 tables, got {final_tables}"

    # Check specific core tables exist
    core_tables = [
        "users",
        "teams",
        "memberships",
        "projects",
        "jobs",
        "job_events",
        "asset_files",
        "webhooks",
        "api_keys",
    ]
    for table in core_tables:
        exists = await framework.check_table_exists(table)
        assert exists, f"Table {table} should exist after migrations"


@pytest.mark.asyncio
async def test_happy_02_migration_status_tracking(migration_framework):
    """HAPPY-02: Migration status tracking works correctly"""
    framework = migration_framework

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], "Migrations should succeed"

    # Check migration tracking table exists
    tracking_exists = await framework.check_table_exists("_sqlx_migrations")
    assert tracking_exists, "Migration tracking table should exist"

    # Check migration records
    migrations = await framework.get_migration_status()
    assert len(migrations) >= 2, "Should have at least 2 migration records"

    # Verify all migrations marked as successful
    for migration in migrations:
        assert migration["success"], (
            f"Migration {migration['version']} should be successful"
        )


@pytest.mark.asyncio
async def test_happy_03_business_logic_triggers(migration_framework):
    """HAPPY-03: Business logic triggers function correctly"""
    framework = migration_framework

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], "Migrations should succeed"

    # Test updated_at trigger works
    user_id = await framework.conn.fetchval("""
        INSERT INTO users (email, name, tier)
        VALUES ('test@example.com', 'Test User', 'starter')
        RETURNING id
    """)

    # Get initial timestamp
    initial_timestamp = await framework.conn.fetchval(
        "SELECT updated_at FROM users WHERE id = $1", user_id
    )

    # Wait a moment and update
    await asyncio.sleep(0.1)
    await framework.conn.execute(
        "UPDATE users SET name = 'Updated Name' WHERE id = $1", user_id
    )

    # Check timestamp was updated
    new_timestamp = await framework.conn.fetchval(
        "SELECT updated_at FROM users WHERE id = $1", user_id
    )

    assert new_timestamp > initial_timestamp, "updated_at should be triggered on update"


# EDGE CASE TESTS


@pytest.mark.asyncio
async def test_edge_01_migration_idempotency(migration_framework):
    """EDGE-01: Migrations are idempotent (can run multiple times)"""
    framework = migration_framework

    # Run migrations first time
    result1 = await framework.run_migrations()
    assert result1["success"], "First migration run should succeed"

    table_count_1 = await framework.get_table_count()

    # Run migrations second time (should be safe)
    result2 = await framework.run_migrations()
    assert result2["success"], "Second migration run should succeed (idempotent)"

    table_count_2 = await framework.get_table_count()
    assert table_count_1 == table_count_2, "Table count should be same after second run"


# ERROR CONDITION TESTS


@pytest.mark.asyncio
async def test_error_01_database_connection_failure():
    """ERROR-01: Handle database connection failures gracefully"""
    framework = MigrationTestFramework()

    # Don't set up test database - use invalid URL
    framework.test_db_url = "postgresql://invalid:invalid@localhost:9999/nonexistent"  # pragma: allowlist secret

    result = await framework.run_migrations()
    assert not result["success"], "Should fail with invalid database URL"
    assert (
        "connection" in result.get("stderr", "").lower()
        or "connection" in result.get("error", "").lower()
    ), "Should report connection error"


# INVARIANT TESTS


@pytest.mark.asyncio
async def test_inv_01_user_credit_constraints(migration_framework):
    """INV-01: User credit constraints always enforced"""
    framework = migration_framework

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], "Migrations should succeed"

    # Test: User credits cannot be negative
    violation = await framework.test_invariant_violation("""
        INSERT INTO users (email, name, tier, credits)
        VALUES ('test@example.com', 'Test User', 'starter', -100)
    """)
    assert violation, "Should prevent negative user credits"


@pytest.mark.asyncio
async def test_inv_02_team_ownership_rules(migration_framework):
    """INV-02: Team ownership rules enforced"""
    framework = migration_framework

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], "Migrations should succeed"

    # Create test data
    user_id = await framework.conn.fetchval("""
        INSERT INTO users (email, name, tier)
        VALUES ('owner@example.com', 'Owner', 'creator')
        RETURNING id
    """)

    team_id = await framework.conn.fetchval("""
        INSERT INTO teams (name, slug)
        VALUES ('Test Team', 'test-team')
        RETURNING id
    """)

    membership_id = await framework.conn.fetchval(
        """
        INSERT INTO memberships (team_id, user_id, role)
        VALUES ($1, $2, 'owner')
        RETURNING id
    """,
        team_id,
        user_id,
    )

    # Test: Cannot delete last owner (should be prevented by trigger)
    violation = await framework.test_invariant_violation(
        "DELETE FROM memberships WHERE id = $1", [membership_id]
    )
    assert violation, "Should prevent deletion of last owner"


@pytest.mark.asyncio
async def test_inv_03_job_concurrency_limits(migration_framework):
    """INV-03: Job concurrency limits enforced"""
    framework = migration_framework

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], "Migrations should succeed"

    # Create starter user
    user_id = await framework.conn.fetchval("""
        INSERT INTO users (email, name, tier)
        VALUES ('starter@example.com', 'Starter User', 'starter')
        RETURNING id
    """)

    # Create first job (should succeed)
    job1_id = await framework.conn.fetchval(
        """
        INSERT INTO jobs (owner, triggered_by, status, spec_snapshot)
        VALUES ($1, $2, 'queued', '{}')
        RETURNING id
    """,
        f"framecast:user:{user_id}",
        user_id,
    )

    assert job1_id is not None, "First job should be created successfully"

    # Try to create second concurrent job (should fail for starter)
    violation = await framework.test_invariant_violation(
        """
        INSERT INTO jobs (owner, triggered_by, status, spec_snapshot)
        VALUES ($1, $2, 'queued', '{}')
    """,
        [f"framecast:user:{user_id}", user_id],
    )

    assert violation, "Should prevent starter user from having >1 concurrent job"


@pytest.mark.asyncio
async def test_inv_04_urn_validation(migration_framework):
    """INV-04: URN validation working correctly"""
    framework = migration_framework

    # Run migrations
    result = await framework.run_migrations()
    assert result["success"], "Migrations should succeed"

    # Create starter user
    user_id = await framework.conn.fetchval("""
        INSERT INTO users (email, name, tier)
        VALUES ('starter@example.com', 'Starter User', 'starter')
        RETURNING id
    """)

    # Test: Starter user cannot have team API key
    violation = await framework.test_invariant_violation(
        """
        INSERT INTO api_keys (user_id, owner, name, key_prefix, key_hash)
        VALUES ($1, 'framecast:team:some-team', 'Team Key', 'sk_test_', 'hash123')
    """,
        [user_id],
    )

    assert violation, "Should prevent starter user from having team API key"


# PERFORMANCE TESTS


@pytest.mark.asyncio
async def test_perf_migration_timing(migration_framework):
    """PERF-DB-01: Migration completes within reasonable time"""
    framework = migration_framework

    start_time = time.time()
    result = await framework.run_migrations()
    end_time = time.time()

    assert result["success"], "Migrations should succeed"

    duration = end_time - start_time
    assert duration < 30, f"Migrations took {duration}s, should complete within 30s"


if __name__ == "__main__":
    # Run tests with pytest
    pytest.main([__file__, "-v"])
