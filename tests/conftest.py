"""
pytest configuration and shared fixtures for Framecast tests
Following Rule 2: Tests Before Code compliance
"""

import asyncio
import logging
import os

import pytest

# Configure logging for tests
logging.basicConfig(level=logging.INFO)

# Test configuration
TEST_DATABASE_URL = os.getenv(
    "TEST_DATABASE_URL",
    "postgresql://postgres:dev-password-framecast@localhost:5432/framecast_test",
)


def pytest_configure(config):
    """Configure pytest with custom markers"""
    config.addinivalue_line(
        "markers", "slow: marks tests as slow (deselect with '-m \"not slow\"')"
    )
    config.addinivalue_line("markers", "integration: marks tests as integration tests")
    config.addinivalue_line("markers", "performance: marks tests as performance tests")
    config.addinivalue_line(
        "markers", "requires_database: marks tests that need database connection"
    )


@pytest.fixture(scope="session")
def event_loop():
    """Create an instance of the default event loop for the test session."""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()


@pytest.fixture(scope="session")
async def verify_test_database():
    """Verify test database is available before running tests"""
    try:
        import asyncpg

        # Try to connect to base database (not test-specific database)
        base_url = TEST_DATABASE_URL.rsplit("/", 1)[0] + "/postgres"
        conn = await asyncpg.connect(base_url)
        await conn.close()
        return True
    except Exception as e:
        pytest.skip(f"Test database not available: {e}")


@pytest.fixture(autouse=True)
async def cleanup_test_databases():
    """Cleanup any leftover test databases after tests"""
    yield  # Run the test

    # Cleanup after test
    try:
        import asyncpg

        base_url = TEST_DATABASE_URL.rsplit("/", 1)[0] + "/postgres"
        conn = await asyncpg.connect(base_url)

        # Find and drop any test databases that might be left behind
        test_dbs = await conn.fetch(
            """
            SELECT datname FROM pg_database
            WHERE datname LIKE 'test_framecast_%'
        """
        )

        for db in test_dbs:
            try:
                await conn.execute(f"DROP DATABASE IF EXISTS {db['datname']}")
            except:
                pass  # Database might be in use

        await conn.close()
    except:
        pass  # Cleanup is best effort


@pytest.fixture()
def mock_environment(monkeypatch):
    """Mock environment variables for testing"""
    test_env = {
        "DATABASE_URL": TEST_DATABASE_URL,
        "AWS_REGION": "us-east-1",
        "S3_BUCKET_OUTPUTS": "test-framecast-outputs",
        "S3_BUCKET_ASSETS": "test-framecast-assets",
        "LOCALSTACK_ENDPOINT": "http://localhost:4566",
        "INNGEST_ENDPOINT": "http://localhost:8288",
    }

    for key, value in test_env.items():
        monkeypatch.setenv(key, value)

    return test_env
