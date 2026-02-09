"""Character Artifact Flow E2E Tests.

Tests the full character artifact lifecycle (11 stories):
  - Direct creation (CF01-CF04)
  - Conversation generation (CF05-CF07)
  - Render endpoint (CF08-CF11)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    create_character,
    create_conversation,
    create_storyboard,
    send_message,
)


@pytest.mark.character
class TestCharacterFlowE2E:
    """Character artifact flow end-to-end tests."""

    # -----------------------------------------------------------------------
    # Direct Creation (CF01-CF04)
    # -----------------------------------------------------------------------

    async def test_cf01_create_character_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF01: POST /v1/artifacts/characters -> 201, kind=character, status=ready."""
        invitee = seed_users.invitee

        artifact = await create_character(http_client, invitee.auth_headers())
        assert artifact["kind"] == "character"
        assert artifact["status"] == "ready"

    async def test_cf02_character_spec_preserved(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF02: Character spec prompt + name preserved in response."""
        invitee = seed_users.invitee

        spec = {"prompt": "A fierce dragon", "name": "Draco"}
        artifact = await create_character(
            http_client, invitee.auth_headers(), spec=spec
        )
        assert artifact["spec"]["prompt"] == "A fierce dragon"
        assert artifact["spec"]["name"] == "Draco"

    async def test_cf03_missing_prompt_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF03: Spec without 'prompt' -> 400/422."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/artifacts/characters",
            json={"spec": {"name": "No Prompt"}},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for missing prompt, got {resp.status_code} {resp.text}"
        )

    async def test_cf04_empty_prompt_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF04: Empty prompt -> 400/422."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/artifacts/characters",
            json={"spec": {"prompt": ""}},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for empty prompt, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Conversation Generation (CF05-CF07)
    # -----------------------------------------------------------------------

    async def test_cf05_conversation_generates_character(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF05: Send 'create a character' -> assistant message has artifacts."""
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, invitee.auth_headers())
        result = await send_message(
            http_client, invitee.auth_headers(), conv["id"], "create a character"
        )

        assistant = result["assistant_message"]
        assert assistant["artifacts"] is not None, (
            "Expected artifacts in assistant message"
        )
        assert len(assistant["artifacts"]) > 0, "Expected at least one artifact"
        assert assistant["artifacts"][0]["kind"] == "character"

    async def test_cf06_conversation_artifact_exists_in_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF06: GET /v1/artifacts -> find character artifact with source=conversation."""
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, invitee.auth_headers())
        await send_message(
            http_client, invitee.auth_headers(), conv["id"], "create a character"
        )

        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        artifacts = resp.json()
        character_artifacts = [
            a
            for a in artifacts
            if a["kind"] == "character" and a["source"] == "conversation"
        ]
        assert len(character_artifacts) > 0, (
            "Expected conversation-sourced character artifact"
        )

    async def test_cf07_conversation_artifact_has_conversation_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF07: Conversation artifact has conversation_id matching."""
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, invitee.auth_headers())
        await send_message(
            http_client, invitee.auth_headers(), conv["id"], "create a character"
        )

        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        artifacts = resp.json()
        character_artifacts = [
            a
            for a in artifacts
            if a["kind"] == "character" and a["source"] == "conversation"
        ]
        assert len(character_artifacts) > 0
        assert character_artifacts[0]["conversation_id"] == conv["id"]

    # -----------------------------------------------------------------------
    # Render Endpoint (CF08-CF11)
    # -----------------------------------------------------------------------

    async def test_cf08_render_character_creates_image(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF08: POST /v1/artifacts/:id/render -> 201, kind=image, status=pending."""
        invitee = seed_users.invitee

        character = await create_character(http_client, invitee.auth_headers())
        character_id = character["id"]

        resp = await http_client.post(
            f"/v1/artifacts/{character_id}/render",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201, f"render failed: {resp.status_code} {resp.text}"
        image = resp.json()
        assert image["kind"] == "image"
        assert image["status"] == "pending"

    async def test_cf09_render_non_character_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF09: Render storyboard -> 400/422."""
        invitee = seed_users.invitee

        storyboard = await create_storyboard(http_client, invitee.auth_headers())
        storyboard_id = storyboard["id"]

        resp = await http_client.post(
            f"/v1/artifacts/{storyboard_id}/render",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for non-character render, got {resp.status_code} {resp.text}"
        )

    async def test_cf10_render_nonexistent_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF10: Render nonexistent UUID -> 404."""
        invitee = seed_users.invitee

        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            f"/v1/artifacts/{fake_id}/render",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 404

    async def test_cf11_render_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CF11: Render no auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            f"/v1/artifacts/{fake_id}/render",
        )
        assert resp.status_code == 401
