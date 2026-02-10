"""JIT (Just-In-Time) User Provisioning E2E Tests.

Tests automatic user creation on first authenticated request (6 stories):
  - JIT-01: First request auto-creates user with correct defaults
  - JIT-02: Second request returns same user (idempotent)
  - JIT-03: JWT without email claim returns 401 MISSING_EMAIL
  - JIT-04: Concurrent first requests all succeed with same user
  - JIT-05: JIT user upgrades to creator, auto-team created
  - JIT-06: JIT user creates conversation, sends message, gets character artifact
"""

import asyncio
import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    create_conversation,
    generate_jit_credentials,
    generate_jit_credentials_no_email,
    send_message,
)


@pytest.mark.jit
class TestSignupJitE2E:
    """JIT user provisioning end-to-end tests."""

    # -------------------------------------------------------------------
    # JIT-01: First request auto-creates user
    # -------------------------------------------------------------------

    async def test_jit01_first_request_creates_user(
        self,
        http_client: httpx.AsyncClient,
    ):
        """JIT-01: GET /v1/account with new JWT auto-creates user with starter tier."""
        user_id, email, headers = generate_jit_credentials()

        resp = await http_client.get("/v1/account", headers=headers)
        assert resp.status_code == 200, (
            f"JIT provisioning failed: {resp.status_code} {resp.text}"
        )

        account = resp.json()
        assert account["id"] == user_id
        assert account["email"] == email
        assert account["tier"] == "starter"
        assert account["credits"] == 0
        assert account["ephemeral_storage_bytes"] == 0
        assert account["upgraded_at"] is None
        assert "created_at" in account
        assert "updated_at" in account

    # -------------------------------------------------------------------
    # JIT-02: Second request returns same user (idempotent)
    # -------------------------------------------------------------------

    async def test_jit02_second_request_returns_same_user(
        self,
        http_client: httpx.AsyncClient,
    ):
        """JIT-02: Repeated GET /v1/account returns the same user (idempotent)."""
        user_id, email, headers = generate_jit_credentials()

        # First request — creates user
        resp1 = await http_client.get("/v1/account", headers=headers)
        assert resp1.status_code == 200
        account1 = resp1.json()

        # Second request — returns same user
        resp2 = await http_client.get("/v1/account", headers=headers)
        assert resp2.status_code == 200
        account2 = resp2.json()

        assert account1["id"] == account2["id"]
        assert account1["email"] == account2["email"]
        assert account1["created_at"] == account2["created_at"]

    # -------------------------------------------------------------------
    # JIT-03: JWT without email claim returns 401
    # -------------------------------------------------------------------

    async def test_jit03_jwt_without_email_returns_401(
        self,
        http_client: httpx.AsyncClient,
    ):
        """JIT-03: JWT without email claim returns 401 MISSING_EMAIL."""
        _, headers = generate_jit_credentials_no_email()

        resp = await http_client.get("/v1/account", headers=headers)
        assert resp.status_code == 401, (
            f"Expected 401 for missing email, got {resp.status_code} {resp.text}"
        )

        error = resp.json()["error"]
        assert error["code"] == "MISSING_EMAIL"

    # -------------------------------------------------------------------
    # JIT-04: Concurrent first requests all succeed
    # -------------------------------------------------------------------

    async def test_jit04_concurrent_first_requests_all_succeed(
        self,
        http_client: httpx.AsyncClient,
    ):
        """JIT-04: 5 concurrent requests for a new user all return 200 with same user."""
        user_id, email, headers = generate_jit_credentials()

        async def make_request() -> httpx.Response:
            return await http_client.get("/v1/account", headers=headers)

        responses = await asyncio.gather(*[make_request() for _ in range(5)])

        for resp in responses:
            assert resp.status_code == 200, (
                f"Concurrent JIT request failed: {resp.status_code} {resp.text}"
            )

        accounts = [r.json() for r in responses]
        ids = {a["id"] for a in accounts}
        emails = {a["email"] for a in accounts}

        assert ids == {user_id}, f"Expected single user ID, got {ids}"
        assert emails == {email}, f"Expected single email, got {emails}"

    # -------------------------------------------------------------------
    # JIT-05: JIT user upgrades to creator, auto-team created
    # -------------------------------------------------------------------

    async def test_jit05_jit_user_upgrades_to_creator(
        self,
        http_client: httpx.AsyncClient,
    ):
        """JIT-05: JIT-provisioned user upgrades to creator; auto-team created."""
        _, _, headers = generate_jit_credentials()

        # 1. First request provisions starter user
        resp = await http_client.get("/v1/account", headers=headers)
        assert resp.status_code == 200
        assert resp.json()["tier"] == "starter"

        # 2. Upgrade to creator
        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=headers,
        )
        assert resp.status_code == 200, (
            f"Upgrade failed: {resp.status_code} {resp.text}"
        )

        # 3. Verify creator tier + upgraded_at set
        resp = await http_client.get("/v1/account", headers=headers)
        assert resp.status_code == 200
        account = resp.json()
        assert account["tier"] == "creator"
        assert account["upgraded_at"] is not None

        # 4. Auto-team should exist
        resp = await http_client.get("/v1/teams", headers=headers)
        assert resp.status_code == 200
        teams = resp.json()
        assert len(teams) >= 1, "Expected at least 1 auto-created team after upgrade"

    # -------------------------------------------------------------------
    # JIT-06: JIT user full conversation flow
    # -------------------------------------------------------------------

    async def test_jit06_jit_user_conversation_to_character(
        self,
        http_client: httpx.AsyncClient,
    ):
        """JIT-06: JIT user creates conversation, sends message, gets character artifact."""
        _, _, headers = generate_jit_credentials()

        # 1. First request provisions starter user
        resp = await http_client.get("/v1/account", headers=headers)
        assert resp.status_code == 200
        assert resp.json()["tier"] == "starter"

        # 2. Create conversation (starter can create conversations)
        conv = await create_conversation(http_client, headers)
        conv_id = conv["id"]

        # 3. Send message that triggers character generation
        result = await send_message(http_client, headers, conv_id, "create a character")
        assistant = result["assistant_message"]
        assert assistant["artifacts"] is not None
        assert len(assistant["artifacts"]) > 0
        assert assistant["artifacts"][0]["kind"] == "character"
        character_id = assistant["artifacts"][0]["id"]

        # 4. Verify character artifact exists and links to conversation
        resp = await http_client.get(f"/v1/artifacts/{character_id}", headers=headers)
        assert resp.status_code == 200
        artifact = resp.json()
        assert artifact["source"] == "conversation"
        assert artifact["conversation_id"] == conv_id
