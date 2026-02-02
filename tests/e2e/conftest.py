"""
E2E Test Configuration and Fixtures

Provides user personas, service mocking, and test infrastructure for comprehensive
end-to-end testing of the Framecast API.

Supports two modes:
- Mocked mode (TEST_MODE=mocked): Fast tests with mocked external services
- Real mode (TEST_MODE=real): Integration tests with actual services
"""

import asyncio
import tempfile
from pathlib import Path
from typing import Any, AsyncGenerator, Dict, List

import httpx
import pytest
import respx
from faker import Faker
from pydantic import BaseModel, ConfigDict
from pydantic_settings import BaseSettings


# Test environment configuration
class TestConfig(BaseSettings):
    """Configuration for E2E tests loaded from environment variables."""

    # Test mode: "mocked" or "real"
    test_mode: str = "mocked"

    # API base URL
    api_base_url: str = "http://localhost:3000"

    # Database settings
    database_url: str = "postgresql://postgres:password@localhost:5432/framecast_test"

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

    model_config = ConfigDict(env_prefix="TEST_", env_file=".env.test")


# User personas for testing
class UserPersona(BaseModel):
    """Represents a test user with specific characteristics."""

    id: str
    email: str
    name: str
    tier: str  # "visitor", "starter", "creator"
    credits: int = 0
    team_memberships: List[str] = []
    owned_teams: List[str] = []
    api_keys: List[str] = []

    def to_auth_token(self) -> str:
        """Generate a mock JWT token for this user."""
        import base64
        import json

        payload = {
            "sub": self.id,
            "email": self.email,
            "aud": "authenticated",
            "role": "authenticated",
            "framecast_tier": self.tier,
            "exp": 9999999999,  # Far future expiry
        }

        # Simple mock JWT (not cryptographically valid, for testing only)
        header = base64.b64encode(
            json.dumps({"alg": "HS256", "typ": "JWT"}).encode()
        ).decode()
        payload_encoded = base64.b64encode(json.dumps(payload).encode()).decode()
        signature = "mock-signature"

        return f"{header}.{payload_encoded}.{signature}"


# Standard user personas for testing
@pytest.fixture()
def visitor_user() -> UserPersona:
    """A visitor user (not authenticated)."""
    fake = Faker()
    return UserPersona(
        id="usr_visitor_test",
        email=fake.email(),
        name=fake.name(),
        tier="visitor",
    )


@pytest.fixture()
def starter_user() -> UserPersona:
    """A starter tier user with some credits."""
    fake = Faker()
    return UserPersona(
        id="usr_starter_test",
        email=fake.email(),
        name=fake.name(),
        tier="starter",
        credits=1000,  # 10 dollars worth
        api_keys=["ak_starter_test_key"],
    )


@pytest.fixture()
def creator_user() -> UserPersona:
    """A creator tier user with team memberships."""
    fake = Faker()
    return UserPersona(
        id="usr_creator_test",
        email=fake.email(),
        name=fake.name(),
        tier="creator",
        credits=5000,  # 50 dollars worth
        team_memberships=["tm_test_team_1"],
        owned_teams=["tm_test_team_owned"],
        api_keys=["ak_creator_test_key"],
    )


@pytest.fixture()
def team_owner() -> UserPersona:
    """A creator user who owns multiple teams."""
    fake = Faker()
    return UserPersona(
        id="usr_team_owner_test",
        email=fake.email(),
        name=fake.name(),
        tier="creator",
        credits=10000,  # 100 dollars worth
        owned_teams=["tm_team_1", "tm_team_2"],
        api_keys=["ak_team_owner_key"],
    )


@pytest.fixture()
def team_member() -> UserPersona:
    """A creator user who is a member of teams but doesn't own any."""
    fake = Faker()
    return UserPersona(
        id="usr_team_member_test",
        email=fake.email(),
        name=fake.name(),
        tier="creator",
        credits=2000,  # 20 dollars worth
        team_memberships=["tm_team_1", "tm_team_2"],
        api_keys=["ak_team_member_key"],
    )


# Configuration and test environment
@pytest.fixture(scope="session")
def test_config() -> TestConfig:
    """Test configuration loaded from environment."""
    return TestConfig()


@pytest.fixture(scope="session")
def event_loop():
    """Create an instance of the default event loop for the test session."""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()


# HTTP client for API testing
@pytest.fixture()
async def http_client(
    test_config: TestConfig,
) -> AsyncGenerator[httpx.AsyncClient, None]:
    """HTTP client for making API requests."""
    async with httpx.AsyncClient(
        base_url=test_config.api_base_url,
        timeout=30.0,
        headers={"User-Agent": "Framecast-E2E-Tests/0.0.1-SNAPSHOT"},
    ) as client:
        yield client


@pytest.fixture()
async def authenticated_client(
    http_client: httpx.AsyncClient, starter_user: UserPersona
) -> httpx.AsyncClient:
    """HTTP client with starter user authentication."""
    http_client.headers.update(
        {"Authorization": f"Bearer {starter_user.to_auth_token()}"}
    )
    return http_client


# Mock service infrastructure
@pytest.fixture()
def mock_runpod(test_config: TestConfig):
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


@pytest.fixture()
def mock_anthropic(test_config: TestConfig):
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


@pytest.fixture()
def mock_inngest(test_config: TestConfig):
    """Mock Inngest event API for job orchestration."""
    if test_config.test_mode != "mocked":
        yield None
        return

    with respx.mock:
        respx.post("https://inn.gs/e/test-inngest-key").mock(
            return_value=httpx.Response(200, json={"status": "ok"})
        )
        yield


@pytest.fixture()
async def mock_s3(test_config: TestConfig):
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


# Database utilities
@pytest.fixture()
async def clean_database(test_config: TestConfig):
    """Ensure clean database state for testing."""
    # This will be implemented when we have database layer
    # For now, it's a placeholder
    return
    # Cleanup after test


# Temporary file management
@pytest.fixture()
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
    def video_spec(scene_count: int = 3) -> Dict[str, Any]:
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
    def team_data() -> Dict[str, Any]:
        """Generate valid team creation data."""
        fake = Faker()
        return {
            "name": fake.company(),
            "description": fake.text(max_nb_chars=200),
            "settings": {"default_resolution": "1920x1080", "webhook_url": fake.url()},
        }


@pytest.fixture()
def test_data_factory() -> TestDataFactory:
    """Factory for generating test data."""
    return TestDataFactory()


# Session-level setup and teardown
@pytest.fixture(scope="session", autouse=True)
async def setup_test_environment(test_config: TestConfig):
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
        assert (
            parts[1] == expected_type
        ), f"Expected URN type {expected_type}, got {parts[1]}"


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
    "TestConfig",
    "UserPersona",
    "test_config",
    "http_client",
    "authenticated_client",
    "visitor_user",
    "starter_user",
    "creator_user",
    "team_owner",
    "team_member",
    "mock_runpod",
    "mock_anthropic",
    "mock_inngest",
    "mock_s3",
    "clean_database",
    "temp_asset_file",
    "test_data_factory",
    "TestDataFactory",
    "assert_valid_urn",
    "assert_job_status_valid",
    "assert_user_tier_valid",
    "assert_credits_non_negative",
]
