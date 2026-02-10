"""
E2E Test Configuration and Fixtures

Provides user personas, service clients, and test infrastructure for
end-to-end testing of the Framecast API against a running local stack.
"""

import asyncio
import os
import time
import uuid
from collections.abc import AsyncGenerator
from typing import Any

import asyncpg
import httpx
import jwt
import pytest
from faker import Faker
from pydantic import BaseModel, ConfigDict
from pydantic_settings import BaseSettings
from utils.localstack_email import LocalStackEmailClient


# Test environment configuration
class E2EConfig(BaseSettings):
    """Configuration for E2E tests loaded from environment variables."""

    # API base URL
    local_api_url: str = "http://localhost:3000"

    @property
    def api_base_url(self) -> str:
        """Return the API URL."""
        return self.local_api_url

    # Database settings
    database_url: str = "postgresql://postgres:password@localhost:5432/framecast_test"  # pragma: allowlist secret

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
        secret = os.environ.get("JWT_SECRET", "test-e2e-secret-key-for-ci-only-0")
        return jwt.encode(payload, secret, algorithm="HS256")

    def auth_headers(self) -> dict[str, str]:
        """Return authorization headers for HTTP requests."""
        return {"Authorization": f"Bearer {self.to_auth_token()}"}


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

        from datetime import UTC, datetime

        now_dt = datetime.now(UTC)

        # Upsert owner (Creator tier)
        await conn.execute(
            """
            INSERT INTO users (id, email, name, tier, credits,
                               ephemeral_storage_bytes, upgraded_at, created_at, updated_at)
            VALUES ($1, $2, $3, 'creator', 5000, 0, $4, $4, $4)
            ON CONFLICT (email) DO UPDATE SET
                tier = 'creator', credits = 5000, upgraded_at = $4, updated_at = $4
            """,
            owner_id,
            owner_email,
            "Test Owner",
            now_dt,
        )
        # Re-read the actual ID in case it was an existing row
        row = await conn.fetchrow("SELECT id FROM users WHERE email = $1", owner_email)
        owner_id = row["id"]

        # Upsert invitee (Starter tier — will be auto-upgraded on accept)
        await conn.execute(
            """
            INSERT INTO users (id, email, name, tier, credits,
                               ephemeral_storage_bytes, created_at, updated_at)
            VALUES ($1, $2, $3, 'starter', 1000, 0, $4, $4)
            ON CONFLICT (email) DO UPDATE SET
                tier = 'starter', credits = 1000, upgraded_at = NULL, updated_at = $4
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
            "TRUNCATE jobs, job_events, message_artifacts, messages, artifacts, conversations, "
            "api_keys, invitations, memberships, teams, users CASCADE"
        )
    finally:
        await conn.close()


# System asset seeding for E2E tests
@pytest.fixture
async def seed_system_assets(test_config: E2EConfig):
    """Seed system assets into the database for E2E tests."""
    database_url = os.environ.get("DATABASE_URL", test_config.database_url)
    conn = await asyncpg.connect(database_url)
    try:
        assets = [
            ("asset_sfx_whoosh_01", "sfx", "Whoosh 01", "audio/mpeg", 2048),
            ("asset_ambient_rain_01", "ambient", "Rain 01", "audio/mpeg", 4096),
            ("asset_music_chill_01", "music", "Chill 01", "audio/mpeg", 8192),
            (
                "asset_transition_fade_01",
                "transition",
                "Fade 01",
                "video/mp4",
                16384,
            ),
        ]
        for asset_id, category, name, content_type, size_bytes in assets:
            await conn.execute(
                """
                INSERT INTO system_assets
                    (id, category, name, description, s3_key, content_type,
                     size_bytes, tags, created_at)
                VALUES ($1, $2::system_asset_category, $3, $4, $5, $6, $7, $8, NOW())
                ON CONFLICT (id) DO NOTHING
                """,
                asset_id,
                category,
                name,
                f"Test {name}",
                f"system-assets/{category}/{asset_id}",
                content_type,
                size_bytes,
                ["test"],
            )
        yield assets
        await conn.execute("TRUNCATE system_assets CASCADE")
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


# Test data factories
class TestDataFactory:
    """Factory for generating test data."""

    @staticmethod
    def team_data() -> dict[str, Any]:
        """Generate valid team creation data."""
        fake = Faker()
        return {
            "name": fake.company(),
            "description": fake.text(max_nb_chars=200),
            "settings": {"default_resolution": "1920x1080", "webhook_url": fake.url()},
        }

    @staticmethod
    def conversation_data(
        model: str = "test-model",
        title: str | None = None,
        system_prompt: str | None = None,
    ) -> dict[str, Any]:
        """Generate valid conversation creation data."""
        data: dict[str, Any] = {"model": model}
        if title is not None:
            data["title"] = title
        if system_prompt is not None:
            data["system_prompt"] = system_prompt
        return data

    @staticmethod
    def storyboard_data(
        spec: dict[str, Any] | None = None,
        owner: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Generate valid storyboard creation data."""
        data: dict[str, Any] = {"spec": spec if spec is not None else {"scenes": []}}
        if owner is not None:
            data["owner"] = owner
        if project_id is not None:
            data["project_id"] = project_id
        return data

    @staticmethod
    def character_data(
        spec: dict[str, Any] | None = None,
        owner: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Generate valid character creation data."""
        data: dict[str, Any] = {
            "spec": spec
            if spec is not None
            else {"prompt": "A brave warrior", "name": "Test Character"}
        }
        if owner is not None:
            data["owner"] = owner
        if project_id is not None:
            data["project_id"] = project_id
        return data

    @staticmethod
    def message_data(content: str = "Hello, how are you?") -> dict[str, Any]:
        """Generate valid message send data."""
        return {"content": content}


@pytest.fixture
def test_data_factory() -> TestDataFactory:
    """Factory for generating test data."""
    return TestDataFactory()


# Session-level setup and teardown
@pytest.fixture(scope="session", autouse=True)
async def setup_test_environment(test_config: E2EConfig):
    """Set up the test environment before running tests."""
    print(f"\n🧪 Setting up E2E test environment (API: {test_config.api_base_url})")

    yield

    print("\n🧹 Cleaning up test environment")


# Utility functions for tests
def assert_credits_non_negative(credits: int) -> None:
    """Assert that credits are non-negative (business invariant)."""
    assert credits >= 0, f"Credits cannot be negative: {credits}"


# Helper functions for multi-step E2E flows
async def create_conversation(
    client: httpx.AsyncClient,
    headers: dict[str, str],
    model: str = "test-model",
    title: str | None = None,
    system_prompt: str | None = None,
) -> dict[str, Any]:
    """Create a conversation and return the response JSON."""
    data = TestDataFactory.conversation_data(model, title, system_prompt)
    resp = await client.post("/v1/conversations", json=data, headers=headers)
    assert resp.status_code == 201, (
        f"create_conversation failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def send_message(
    client: httpx.AsyncClient,
    headers: dict[str, str],
    conversation_id: str,
    content: str = "Hello",
) -> dict[str, Any]:
    """Send a message and return the response JSON (user_message + assistant_message)."""
    resp = await client.post(
        f"/v1/conversations/{conversation_id}/messages",
        json=TestDataFactory.message_data(content),
        headers=headers,
    )
    assert resp.status_code == 201, (
        f"send_message failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def create_storyboard(
    client: httpx.AsyncClient,
    headers: dict[str, str],
    spec: dict[str, Any] | None = None,
    owner: str | None = None,
    project_id: str | None = None,
) -> dict[str, Any]:
    """Create a storyboard artifact and return the response JSON."""
    data = TestDataFactory.storyboard_data(spec, owner, project_id)
    resp = await client.post("/v1/artifacts/storyboards", json=data, headers=headers)
    assert resp.status_code == 201, (
        f"create_storyboard failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def create_character(
    client: httpx.AsyncClient,
    headers: dict[str, str],
    spec: dict[str, Any] | None = None,
    owner: str | None = None,
    project_id: str | None = None,
) -> dict[str, Any]:
    """Create a character artifact and return the response JSON."""
    data = TestDataFactory.character_data(spec, owner, project_id)
    resp = await client.post("/v1/artifacts/characters", json=data, headers=headers)
    assert resp.status_code == 201, (
        f"create_character failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


def generate_jit_credentials(
    email: str | None = None,
) -> tuple[str, str, dict[str, str]]:
    """Generate auth credentials for a user that does NOT exist in the database.

    Returns (user_id, email, auth_headers) for use in JIT provisioning tests.
    The JWT is valid but the user has no DB row — the API should auto-create it.
    """
    user_id = str(uuid.uuid4())
    user_email = email or f"jit-{user_id[:8]}@test.com"
    persona = UserPersona(
        user_id=user_id,
        email=user_email,
        name="JIT User",
        tier="starter",
    )
    return user_id, user_email, persona.auth_headers()


def generate_jit_credentials_no_email() -> tuple[str, dict[str, str]]:
    """Generate auth credentials with a JWT that has no email claim.

    Returns (user_id, auth_headers) where the JWT omits the email field.
    """
    user_id = str(uuid.uuid4())
    payload = {
        "sub": user_id,
        "aud": "authenticated",
        "role": "authenticated",
        "iat": int(time.time()),
        "exp": int(time.time()) + 3600,
    }
    secret = os.environ.get("JWT_SECRET", "test-e2e-secret-key-for-ci-only-0")
    token = jwt.encode(payload, secret, algorithm="HS256")
    headers = {"Authorization": f"Bearer {token}"}
    return user_id, headers


async def create_ephemeral_job(
    client: httpx.AsyncClient,
    headers: dict[str, str],
    spec: dict[str, Any] | None = None,
    options: dict[str, Any] | None = None,
    idempotency_key: str | None = None,
    owner: str | None = None,
) -> dict[str, Any]:
    """Create an ephemeral job and return the response JSON."""
    data: dict[str, Any] = {
        "spec": spec if spec is not None else {"prompt": "A brave warrior"}
    }
    if options is not None:
        data["options"] = options
    if idempotency_key is not None:
        data["idempotency_key"] = idempotency_key
    if owner is not None:
        data["owner"] = owner
    resp = await client.post("/v1/generate", json=data, headers=headers)
    assert resp.status_code == 201, (
        f"create_ephemeral_job failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def create_render_job(
    client: httpx.AsyncClient,
    headers: dict[str, str],
    artifact_id: str,
) -> dict[str, Any]:
    """Render an artifact and return the response JSON (job + artifact)."""
    resp = await client.post(f"/v1/artifacts/{artifact_id}/render", headers=headers)
    assert resp.status_code == 201, (
        f"create_render_job failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def trigger_callback(
    client: httpx.AsyncClient,
    job_id: str,
    event: str,
    output: dict[str, Any] | None = None,
    error: dict[str, Any] | None = None,
    failure_type: str | None = None,
    progress_percent: float | None = None,
    output_size_bytes: int | None = None,
) -> httpx.Response:
    """Send a job callback event (internal endpoint, no auth)."""
    payload: dict[str, Any] = {"job_id": job_id, "event": event}
    if output is not None:
        payload["output"] = output
    if output_size_bytes is not None:
        payload["output_size_bytes"] = output_size_bytes
    if error is not None:
        payload["error"] = error
    if failure_type is not None:
        payload["failure_type"] = failure_type
    if progress_percent is not None:
        payload["progress_percent"] = progress_percent
    return await client.post("/internal/jobs/callback", json=payload)


async def complete_job(
    client: httpx.AsyncClient,
    job_id: str,
    output: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Send started + completed callbacks. Returns the final job response."""
    resp = await trigger_callback(client, job_id, "started")
    assert resp.status_code == 200, (
        f"started callback failed: {resp.status_code} {resp.text}"
    )
    resp = await trigger_callback(
        client,
        job_id,
        "completed",
        output=output or {"url": "https://example.com/output.png"},
        output_size_bytes=12345,
    )
    assert resp.status_code == 200, (
        f"completed callback failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def fail_job(
    client: httpx.AsyncClient,
    job_id: str,
    error: dict[str, Any] | None = None,
    failure_type: str = "system",
) -> dict[str, Any]:
    """Send started + failed callbacks. Returns the final job response."""
    resp = await trigger_callback(client, job_id, "started")
    assert resp.status_code == 200, (
        f"started callback failed: {resp.status_code} {resp.text}"
    )
    resp = await trigger_callback(
        client,
        job_id,
        "failed",
        error=error or {"message": "Something went wrong"},
        failure_type=failure_type,
    )
    assert resp.status_code == 200, (
        f"failed callback failed: {resp.status_code} {resp.text}"
    )
    return resp.json()


async def configure_mock_render(
    client: httpx.AsyncClient,
    outcome: str = "complete",
    delay_ms: int = 50,
    progress_steps: list[float] | None = None,
) -> None:
    """Configure mock render behavior."""
    payload: dict[str, Any] = {"outcome": outcome, "delay_ms": delay_ms}
    if progress_steps is not None:
        payload["progress_steps"] = progress_steps
    resp = await client.post("/internal/mock/render/configure", json=payload)
    assert resp.status_code == 200, (
        f"configure_mock_render failed: {resp.status_code} {resp.text}"
    )


async def reset_mock_render(client: httpx.AsyncClient) -> None:
    """Reset mock render behavior and history."""
    resp = await client.post("/internal/mock/render/reset")
    assert resp.status_code == 200, (
        f"reset_mock_render failed: {resp.status_code} {resp.text}"
    )


async def get_mock_render_history(client: httpx.AsyncClient) -> list[dict[str, Any]]:
    """Get mock render request history."""
    resp = await client.get("/internal/mock/render/history")
    assert resp.status_code == 200
    return resp.json()


# Export commonly used fixtures and utilities
__all__ = [
    "E2EConfig",
    "UserPersona",
    "SeededUsers",
    "test_config",
    "http_client",
    "localstack_email_client",
    "seed_users",
    "seed_system_assets",
    "test_data_factory",
    "TestDataFactory",
    "assert_credits_non_negative",
    "create_conversation",
    "send_message",
    "create_storyboard",
    "create_character",
    "generate_jit_credentials",
    "generate_jit_credentials_no_email",
    "create_ephemeral_job",
    "create_render_job",
    "trigger_callback",
    "complete_job",
    "fail_job",
    "configure_mock_render",
    "reset_mock_render",
    "get_mock_render_history",
]
