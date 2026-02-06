"""
E2E Test Configuration and Fixtures

Provides user personas, service mocking, and test infrastructure for comprehensive
end-to-end testing of the Framecast API.

Supports two modes:
- Mocked mode (TEST_MODE=mocked): Fast tests with mocked external services
- Real mode (TEST_MODE=real): Integration tests with actual services
"""

import asyncio
import os
import tempfile
import time
import uuid
from collections.abc import AsyncGenerator
from pathlib import Path
from typing import Any

import asyncpg
import httpx
import jwt
import pytest
import respx
from faker import Faker
from pydantic import BaseModel, ConfigDict
from pydantic_settings import BaseSettings
from utils.localstack_email import LocalStackEmailClient


# Test environment configuration
class E2EConfig(BaseSettings):
    """Configuration for E2E tests loaded from environment variables.

    Supports SAM local testing via USE_SAM_LOCAL=true environment variable.
    When enabled, tests will target the SAM local API at http://localhost:3001
    instead of the local development server at http://localhost:3000.
    """

    # Test mode: "mocked" or "real"
    test_mode: str = "mocked"

    # API base URL (for local development server)
    local_api_url: str = "http://localhost:3000"

    # SAM Local settings
    sam_api_url: str = "http://localhost:3001"
    use_sam_local: bool = False

    @property
    def api_base_url(self) -> str:
        """Return the appropriate API URL based on configuration.

        When USE_SAM_LOCAL=true, returns SAM local URL (port 3001).
        Otherwise, returns local development server URL (port 3000).
        """
        return self.sam_api_url if self.use_sam_local else self.local_api_url

    # Database settings
    database_url: str = "postgresql://postgres:password@localhost:5432/framecast_test"  # pragma: allowlist secret

    # External service URLs (used in real mode)
    supabase_url: str = "http://localhost:54321"
    supabase_anon_key: str = "test-anon-key"
    supabase_service_role_key: str = "test-service-role-key"
    inngest_event_key: str = "test-inngest-key"
    runpod_api_key: str = "test-runpod-key"
    runpod_endpoint_id: str = "test-endpoint-id"
    anthropic_api_key: str = "test-anthropic-key"

    # S3 settings (LocalStack in test)
    s3_bucket_outputs: str = "test-framecast-outputs"
    s3_bucket_assets: str = "test-framecast-assets"
    aws_region: str = "us-east-1"
    s3_endpoint_url: str = "http://localhost:4566"  # LocalStack

    # LocalStack email retrieval settings
    localstack_ses_url: str = "http://localhost:4566"
    email_retrieval_enabled: bool = True
    email_retrieval_timeout: int = 10
    email_cleanup_enabled: bool = True

    model_config = ConfigDict(env_prefix="TEST_", env_file=".env.test")


# User personas for testing
class UserPersona(BaseModel):
    """Represents a test user with specific characteristics."""

    user_id: str  # UUID string
    email: str
    name: str
    tier: str  # "starter", "creator"
    credits: int = 0
    team_memberships: list[str] = []
    owned_teams: list[str] = []
    api_keys: list[str] = []

    def to_auth_token(self) -> str:
        """Generate a proper HS256 JWT token for this user."""
        payload = {
            "sub": self.user_id,
            "email": self.email,
            "aud": "authenticated",
            "role": "authenticated",
            "iat": int(time.time()),
            "exp": int(time.time()) + 3600,
        }
        secret = os.environ.get("JWT_SECRET", "test-e2e-secret-key")
        return jwt.encode(payload, secret, algorithm="HS256")

    def auth_headers(self) -> dict[str, str]:
        """Return authorization headers for HTTP requests."""
        return {"Authorization": f"Bearer {self.to_auth_token()}"}


# Standard user personas for testing
@pytest.fixture
def starter_user() -> UserPersona:
    """A starter tier user with some credits."""
    fake = Faker()
    return UserPersona(
        user_id=str(uuid.uuid4()),
        email=fake.email(),
        name=fake.name(),
        tier="starter",
        credits=1000,
    )


@pytest.fixture
def creator_user() -> UserPersona:
    """A creator tier user with team memberships."""
    fake = Faker()
    return UserPersona(
        user_id=str(uuid.uuid4()),
        email=fake.email(),
        name=fake.name(),
        tier="creator",
        credits=5000,
    )


@pytest.fixture
def team_owner() -> UserPersona:
    """A creator user who owns multiple teams."""
    fake = Faker()
    return UserPersona(
        user_id=str(uuid.uuid4()),
        email=fake.email(),
        name=fake.name(),
        tier="creator",
        credits=10000,
    )


@pytest.fixture
def team_member() -> UserPersona:
    """A creator user who is a member of teams but doesn't own any."""
    fake = Faker()
    return UserPersona(
        user_id=str(uuid.uuid4()),
        email=fake.email(),
        name=fake.name(),
        tier="creator",
        credits=2000,
    )


# Configuration and test environment
@pytest.fixture(scope="session")
def test_config() -> E2EConfig:
    """Test configuration loaded from environment."""
    return E2EConfig()


@pytest.fixture(scope="session")
def event_loop():
    """Create an instance of the default event loop for the test session."""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()


# Database seeding for E2E tests
class SeededUsers:
    """Container for seeded test users."""

    def __init__(self, owner: UserPersona, invitee: UserPersona):
        self.owner = owner
        self.invitee = invitee


@pytest.fixture
async def seed_users(test_config: E2EConfig):
    """Seed test users directly into the database for E2E tests."""
    database_url = os.environ.get("DATABASE_URL", test_config.database_url)
    conn = await asyncpg.connect(database_url)
    try:
        owner_id = uuid.uuid4()
        owner_email = "owner-e2e@test.com"
        invitee_id = uuid.uuid4()
        invitee_email = "invitee-e2e@test.com"

        now = asyncio.get_event_loop().time()
        from datetime import datetime, timezone

        now_dt = datetime.now(timezone.utc)

        # Upsert owner (Creator tier)
        await conn.execute(
            """
            INSERT INTO users (id, email, name, tier, credits,
                               ephemeral_storage_bytes, upgraded_at, created_at, updated_at)
            VALUES ($1, $2, $3, 'creator', 5000, 0, $4, $4, $4)
            ON CONFLICT (email) DO UPDATE SET
                id = $1, tier = 'creator', credits = 5000, upgraded_at = $4, updated_at = $4
            """,
            owner_id,
            owner_email,
            "Test Owner",
            now_dt,
        )
        # Re-read the actual ID in case it was an existing row
        row = await conn.fetchrow(
            "SELECT id FROM users WHERE email = $1", owner_email
        )
        owner_id = row["id"]

        # Upsert invitee (Starter tier — will be auto-upgraded on accept)
        await conn.execute(
            """
            INSERT INTO users (id, email, name, tier, credits,
                               ephemeral_storage_bytes, created_at, updated_at)
            VALUES ($1, $2, $3, 'starter', 1000, 0, $4, $4)
            ON CONFLICT (email) DO UPDATE SET
                id = $1, tier = 'starter', credits = 1000, upgraded_at = NULL, updated_at = $4
            """,
            invitee_id,
            invitee_email,
            "Test Invitee",
            now_dt,
        )
        row = await conn.fetchrow(
            "SELECT id FROM users WHERE email = $1", invitee_email
        )
        invitee_id = row["id"]

        owner = UserPersona(
            user_id=str(owner_id),
            email=owner_email,
            name="Test Owner",
            tier="creator",
            credits=5000,
        )
        invitee = UserPersona(
            user_id=str(invitee_id),
            email=invitee_email,
            name="Test Invitee",
            tier="starter",
            credits=1000,
        )

        yield SeededUsers(owner=owner, invitee=invitee)

        # Cleanup: TRUNCATE bypasses FK constraints and INV-T2 trigger
        await conn.execute(
            "TRUNCATE invitations, memberships, teams, users CASCADE"
        )
    finally:
        await conn.close()


# HTTP client for API testing
@pytest.fixture
async def http_client(
    test_config: E2EConfig,
) -> AsyncGenerator[httpx.AsyncClient, None]:
    """HTTP client for making API requests."""
    async with httpx.AsyncClient(
        base_url=test_config.api_base_url,
        timeout=30.0,
        headers={"User-Agent": "Framecast-E2E-Tests/0.0.1-SNAPSHOT"},
    ) as client:
        yield client


@pytest.fixture
async def authenticated_client(
    http_client: httpx.AsyncClient, starter_user: UserPersona
) -> httpx.AsyncClient:
    """HTTP client with starter user authentication."""
    http_client.headers.update(
        {"Authorization": f"Bearer {starter_user.to_auth_token()}"}
    )
    return http_client


# LocalStack email client for E2E testing
@pytest.fixture
async def localstack_email_client(
    test_config: E2EConfig,
) -> AsyncGenerator[LocalStackEmailClient, None]:
    """LocalStack SES email client for E2E tests."""
    client = LocalStackEmailClient(test_config.localstack_ses_url)
    try:
        yield client
    finally:
        await client.close()


@pytest.fixture
async def email_cleanup(localstack_email_client: LocalStackEmailClient):
    """Clean up emails after test completion."""
    collected_emails = []

    def register_email(email_address: str, message_id: str):
        """Register an email for cleanup after test completion."""
        collected_emails.append((email_address, message_id))

    yield register_email

    # Cleanup after test
    for _email_address, msg_id in collected_emails:
        await localstack_email_client.delete_email(msg_id)


# Mock service infrastructure
@pytest.fixture
def mock_runpod(test_config: E2EConfig):
    """Mock RunPod API for video generation testing."""
    if test_config.test_mode != "mocked":
        yield None
        return

    with respx.mock:
        # Mock job submission
        respx.post(
            f"https://api.runpod.ai/v2/{test_config.runpod_endpoint_id}/run"
        ).mock(
            return_value=httpx.Response(
                200, json={"id": "mock-job-id", "status": "IN_QUEUE"}
            )
        )

        # Mock job status polling
        respx.get(
            f"https://api.runpod.ai/v2/{test_config.runpod_endpoint_id}/status/mock-job-id"
        ).mock(
            return_value=httpx.Response(
                200,
                json={
                    "id": "mock-job-id",
                    "status": "COMPLETED",
                    "output": {
                        "video_url": "https://mock-storage.runpod.ai/video.mp4",
                        "metadata": {
                            "duration": 30.5,
                            "resolution": "1920x1080",
                            "format": "mp4",
                        },
                    },
                },
            )
        )

        yield


@pytest.fixture
def mock_anthropic(test_config: E2EConfig):
    """Mock Anthropic Claude API for AI interactions."""
    if test_config.test_mode != "mocked":
        yield None
        return

    with respx.mock:
        respx.post("https://api.anthropic.com/v1/messages").mock(
            return_value=httpx.Response(
                200,
                json={
                    "id": "msg_mock",
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "text",
                            "text": "This is a mock response from Claude for testing purposes.",
                        }
                    ],
                    "model": "claude-3-sonnet-20240229",
                    "stop_reason": "end_turn",
                    "stop_sequence": None,
                    "usage": {"input_tokens": 10, "output_tokens": 15},
                },
            )
        )
        yield


@pytest.fixture
def mock_inngest(test_config: E2EConfig):
    """Mock Inngest event API for job orchestration."""
    if test_config.test_mode != "mocked":
        yield None
        return

    with respx.mock:
        respx.post("https://inn.gs/e/test-inngest-key").mock(
            return_value=httpx.Response(200, json={"status": "ok"})
        )
        yield


@pytest.fixture
async def mock_s3(test_config: E2EConfig):
    """Mock S3 operations using LocalStack."""
    if test_config.test_mode != "mocked":
        # In real mode, we use actual LocalStack
        yield None
        return

    # For mocked mode, simulate S3 operations
    with respx.mock:
        # Mock presigned URL generation
        respx.get(f"{test_config.s3_endpoint_url}/{test_config.s3_bucket_assets}").mock(
            return_value=httpx.Response(
                200, json={"presigned_url": "https://mock-s3-url"}
            )
        )

        # Mock file uploads
        respx.put("https://mock-s3-url").mock(return_value=httpx.Response(200))

        yield


# Temporary file management
@pytest.fixture
def temp_asset_file():
    """Create a temporary test asset file."""
    with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
        # Create a simple test image
        from PIL import Image

        img = Image.new("RGB", (100, 100), color="red")
        img.save(f, format="JPEG")
        f.flush()

        yield Path(f.name)

        # Cleanup
        Path(f.name).unlink(missing_ok=True)


# Test data factories
class TestDataFactory:
    """Factory for generating test data."""

    @staticmethod
    def video_spec(scene_count: int = 3) -> dict[str, Any]:
        """Generate a valid video specification."""
        fake = Faker()

        scenes = []
        for i in range(scene_count):
            scenes.append(
                {
                    "id": f"scene_{i}",
                    "prompt": fake.sentence(),
                    "duration": 5.0,
                    "assets": [],
                    "style": "cinematic",
                }
            )

        return {
            "title": fake.sentence(nb_words=3),
            "description": fake.text(),
            "scenes": scenes,
            "settings": {"resolution": "1920x1080", "fps": 30, "format": "mp4"},
            "metadata": {"client_version": "e2e-tests", "test_run": True},
        }

    @staticmethod
    def team_data() -> dict[str, Any]:
        """Generate valid team creation data."""
        fake = Faker()
        return {
            "name": fake.company(),
            "description": fake.text(max_nb_chars=200),
            "settings": {"default_resolution": "1920x1080", "webhook_url": fake.url()},
        }


@pytest.fixture
def test_data_factory() -> TestDataFactory:
    """Factory for generating test data."""
    return TestDataFactory()


# Session-level setup and teardown
@pytest.fixture(scope="session", autouse=True)
async def setup_test_environment(test_config: E2EConfig):
    """Set up the test environment before running tests."""
    print(f"\n🧪 Setting up E2E test environment (mode: {test_config.test_mode})")

    if test_config.test_mode == "real":
        print("⚠️  Running in REAL mode - using actual external services")
        # TODO: Verify real services are available
    else:
        print("🎭 Running in MOCKED mode - using service mocks")

    yield

    print("\n🧹 Cleaning up test environment")


# Utility functions for tests
def assert_valid_urn(urn: str, expected_type: str = None) -> None:
    """Assert that a URN is valid and optionally of a specific type."""
    parts = urn.split(":")
    assert len(parts) >= 3, f"Invalid URN format: {urn}"
    assert parts[0] == "framecast", f"URN must start with 'framecast': {urn}"

    if expected_type:
        assert parts[1] == expected_type, (
            f"Expected URN type {expected_type}, got {parts[1]}"
        )


def assert_job_status_valid(status: str) -> None:
    """Assert that a job status is valid."""
    valid_statuses = ["queued", "processing", "completed", "failed", "canceled"]
    assert status in valid_statuses, f"Invalid job status: {status}"


def assert_user_tier_valid(tier: str) -> None:
    """Assert that a user tier is valid."""
    valid_tiers = ["visitor", "starter", "creator"]
    assert tier in valid_tiers, f"Invalid user tier: {tier}"


def assert_credits_non_negative(credits: int) -> None:
    """Assert that credits are non-negative (business invariant)."""
    assert credits >= 0, f"Credits cannot be negative: {credits}"


# Export commonly used fixtures and utilities
__all__ = [
    "E2EConfig",
    "UserPersona",
    "SeededUsers",
    "test_config",
    "http_client",
    "authenticated_client",
    "localstack_email_client",
    "email_cleanup",
    "seed_users",
    "starter_user",
    "creator_user",
    "team_owner",
    "team_member",
    "mock_runpod",
    "mock_anthropic",
    "mock_inngest",
    "mock_s3",
    "temp_asset_file",
    "test_data_factory",
    "TestDataFactory",
    "assert_valid_urn",
    "assert_job_status_valid",
    "assert_user_tier_valid",
    "assert_credits_non_negative",
]
