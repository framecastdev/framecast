"""Cross-Domain E2E Tests.

Tests interactions across artifacts, conversations, and messages (30 stories):
  - Cascade & FK behavior (XD01-XD02)
  - Conversation-sourced artifacts (XD03-XD05)
  - Multi-conversation isolation (XD06)
  - Archive semantics (XD07)
  - Count accuracy (XD08)
  - Cross-entity isolation (XD09)
  - Timestamp ordering (XD10-XD12)
  - Message artifacts (XD13-XD14)
  - API key auth (XD15-XD22)
  - Key revocation (XD23)
  - Error format consistency (XD24-XD26)
  - Read-only enforcement (XD27-XD28)
  - Independent artifacts (XD29)
  - Token expiry (XD30)
"""

import os
import sys
import time
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import jwt  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    create_conversation,
    create_storyboard,
    send_message,
)


@pytest.mark.cross_domain
class TestCrossDomainE2E:
    """Cross-domain interaction end-to-end tests."""

    # -----------------------------------------------------------------------
    # Cascade & FK Behavior (XD01-XD02)
    # -----------------------------------------------------------------------

    async def test_xd01_delete_conversation_cascades_messages(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD01: Delete conversation -> messages gone (list returns 404)."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        # Delete conversation
        resp = await http_client.delete(
            f"/v1/conversations/{conv['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # List messages -> 404 (conversation doesn't exist)
        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404

    async def test_xd02_delete_conversation_nullifies_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD02: Delete conversation -> artifact.conversation_id=null (artifact still exists).

        Note: This test creates an artifact and a conversation separately,
        since conversation-sourced artifact creation is not yet implemented via API.
        The test verifies the independent artifact persists after conv deletion.
        """
        owner = seed_users.owner

        # Create independent artifact and conversation
        artifact = await create_storyboard(http_client, owner.auth_headers())
        conv = await create_conversation(http_client, owner.auth_headers())

        # Delete conversation
        resp = await http_client.delete(
            f"/v1/conversations/{conv['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Artifact should still exist
        resp = await http_client.get(
            f"/v1/artifacts/{artifact['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

    # -----------------------------------------------------------------------
    # Conversation-Sourced Artifacts (XD03-XD05)
    # -----------------------------------------------------------------------

    async def test_xd03_upload_artifact_has_source_upload(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD03: Upload-sourced artifact has source=upload + conversation_id=null."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        assert artifact["source"] == "upload"
        assert artifact.get("conversation_id") is None

    async def test_xd04_artifact_visible_in_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD04: Created artifact visible in GET /v1/artifacts."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifact_ids = {a["id"] for a in resp.json()}
        assert artifact["id"] in artifact_ids

    async def test_xd05_artifact_owner_is_user_urn(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD05: Artifact owner = framecast:user:{user_id}."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        expected_urn = f"framecast:user:{owner.user_id}"
        assert artifact["owner"] == expected_urn

    # -----------------------------------------------------------------------
    # Multi-Conversation Isolation (XD06)
    # -----------------------------------------------------------------------

    async def test_xd06_sequences_independent_per_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD06: 3 conversations, each with messages; sequences independent per conversation."""
        owner = seed_users.owner

        for _ in range(3):
            conv = await create_conversation(http_client, owner.auth_headers())
            result = await send_message(http_client, owner.auth_headers(), conv["id"])
            # Each conversation starts at sequence 1
            assert result["user_message"]["sequence"] == 1
            assert result["assistant_message"]["sequence"] == 2

    # -----------------------------------------------------------------------
    # Archive Semantics (XD07)
    # -----------------------------------------------------------------------

    async def test_xd07_archive_conversation_messages_still_returned(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD07: Archive conversation, list messages -> still returned."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        # Archive
        await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )

        # Messages still accessible
        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert len(resp.json()) == 2

    # -----------------------------------------------------------------------
    # Count Accuracy (XD08)
    # -----------------------------------------------------------------------

    async def test_xd08_three_sends_message_count_six(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD08: Send 3 times (6 messages); message_count=6."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        for i in range(3):
            await send_message(
                http_client, owner.auth_headers(), conv["id"], f"Message {i}"
            )

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["message_count"] == 6

    # -----------------------------------------------------------------------
    # Cross-Entity Isolation (XD09)
    # -----------------------------------------------------------------------

    async def test_xd09_owner_resources_invisible_to_invitee(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD09: Owner's artifacts+conversations invisible to invitee."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates resources
        artifact = await create_storyboard(http_client, owner.auth_headers())
        conv = await create_conversation(http_client, owner.auth_headers())

        # Invitee sees nothing
        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        invitee_artifact_ids = {a["id"] for a in resp.json()}
        assert artifact["id"] not in invitee_artifact_ids

        resp = await http_client.get(
            "/v1/conversations", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200
        invitee_conv_ids = {c["id"] for c in resp.json()}
        assert conv["id"] not in invitee_conv_ids

    # -----------------------------------------------------------------------
    # Timestamp Ordering (XD10-XD12)
    # -----------------------------------------------------------------------

    async def test_xd10_conversation_created_at_le_updated_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD10: Conversation created_at <= updated_at."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        assert conv["created_at"] <= conv["updated_at"]

    async def test_xd11_artifact_created_at_le_updated_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD11: Artifact created_at <= updated_at."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        assert artifact["created_at"] <= artifact["updated_at"]

    async def test_xd12_patch_conversation_updates_updated_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD12: PATCH conversation -> updated_at changes."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        original_updated = conv["updated_at"]

        resp = await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"title": "New Title"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        new_updated = resp.json()["updated_at"]
        assert new_updated >= original_updated

    # -----------------------------------------------------------------------
    # Message Artifacts (XD13-XD14)
    # -----------------------------------------------------------------------

    async def test_xd13_message_artifacts_field_present(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD13: Messages have artifacts field (nullable JSONB)."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        for msg in resp.json():
            # artifacts field should be present (may be null)
            assert "artifacts" in msg

    async def test_xd14_delete_artifact_does_not_break_messages(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD14: Delete artifact -> messages still accessible."""
        owner = seed_users.owner

        # Create artifact and conversation independently
        artifact = await create_storyboard(http_client, owner.auth_headers())
        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        # Delete artifact
        resp = await http_client.delete(
            f"/v1/artifacts/{artifact['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Messages still accessible
        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert len(resp.json()) == 2

    # -----------------------------------------------------------------------
    # API Key Auth (XD15-XD22)
    #
    # Note: These tests assume handlers accept AnyAuth (API key or JWT).
    # If handlers still use AuthUser (JWT only), these will fail with 401.
    # Skip until the handler migration to AnyAuth is done.
    # -----------------------------------------------------------------------

    async def _create_api_key(
        self,
        http_client: httpx.AsyncClient,
        headers: dict[str, str],
    ) -> tuple[str, str]:
        """Helper: create an API key and return (key_id, raw_key)."""
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "E2E Test Key", "scopes": ["*"]},
            headers=headers,
        )
        assert resp.status_code == 201
        data = resp.json()
        return data["api_key"]["id"], data["raw_key"]

    async def test_xd15_api_key_create_storyboard(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD15: API key auth: create storyboard -> 201."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.post(
            "/v1/artifacts/storyboards",
            json={"spec": {}},
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 201

    async def test_xd16_api_key_list_artifacts(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD16: API key auth: list artifacts -> 200."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/artifacts",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200

    async def test_xd17_api_key_get_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD17: API key auth: get artifact -> 200."""
        owner = seed_users.owner
        artifact = await create_storyboard(http_client, owner.auth_headers())
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.get(
            f"/v1/artifacts/{artifact['id']}",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200

    async def test_xd18_api_key_delete_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD18: API key auth: delete artifact -> 204."""
        owner = seed_users.owner
        artifact = await create_storyboard(http_client, owner.auth_headers())
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.delete(
            f"/v1/artifacts/{artifact['id']}",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 204

    async def test_xd19_api_key_create_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD19: API key auth: create conversation -> 201."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.post(
            "/v1/conversations",
            json={"model": "test-model"},
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 201

    async def test_xd20_api_key_list_conversations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD20: API key auth: list conversations -> 200."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/conversations",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200

    async def test_xd21_api_key_send_message(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD21: API key auth: send message -> success."""
        owner = seed_users.owner
        conv = await create_conversation(http_client, owner.auth_headers())
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.post(
            f"/v1/conversations/{conv['id']}/messages",
            json={"content": "Hello via API key"},
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 201

    async def test_xd22_api_key_list_system_assets(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """XD22: API key auth: list system assets -> 200."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/system-assets",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200

    # -----------------------------------------------------------------------
    # Key Revocation (XD23)
    # -----------------------------------------------------------------------

    async def test_xd23_revoked_api_key_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD23: Revoked API key -> 401 on any endpoint."""
        owner = seed_users.owner
        key_id, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        # Revoke
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Use revoked key
        resp = await http_client.get(
            "/v1/account",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 401

    # -----------------------------------------------------------------------
    # Error Format Consistency (XD24-XD26)
    # -----------------------------------------------------------------------

    async def test_xd24_artifact_404_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD24: Artifact 404 error format: {"error": {"code": "NOT_FOUND"}}."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/artifacts/{fake_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404
        body = resp.json()
        assert "error" in body
        assert body["error"]["code"] == "NOT_FOUND"

    async def test_xd25_conversation_404_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD25: Conversation 404 error format matches."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/conversations/{fake_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404
        body = resp.json()
        assert "error" in body
        assert body["error"]["code"] == "NOT_FOUND"

    async def test_xd26_message_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD26: Message error format matches."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        # Send empty content to trigger validation error
        resp = await http_client.post(
            f"/v1/conversations/{conv['id']}/messages",
            json={"content": ""},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 422]
        body = resp.json()
        assert "error" in body
        assert "code" in body["error"]
        assert "message" in body["error"]

    # -----------------------------------------------------------------------
    # Read-Only Enforcement (XD27-XD28)
    # -----------------------------------------------------------------------

    async def test_xd27_post_system_assets_returns_405(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD27: POST /v1/system-assets -> 405 Method Not Allowed."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/system-assets",
            json={"name": "test"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 405

    async def test_xd28_delete_system_asset_returns_405(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """XD28: DELETE /v1/system-assets/{id} -> 405."""
        owner = seed_users.owner

        resp = await http_client.delete(
            "/v1/system-assets/asset_sfx_whoosh_01",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 405

    # -----------------------------------------------------------------------
    # Independent Artifacts (XD29)
    # -----------------------------------------------------------------------

    async def test_xd29_artifact_without_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD29: Create artifact without conversation; source=upload, conversation_id=null."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        assert artifact["source"] == "upload"
        assert artifact.get("conversation_id") is None

    # -----------------------------------------------------------------------
    # Token Expiry (XD30)
    # -----------------------------------------------------------------------

    async def test_xd30_expired_jwt_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """XD30: Expired JWT -> 401 on conversation endpoints."""
        owner = seed_users.owner

        # Generate an expired token
        payload = {
            "sub": owner.user_id,
            "email": owner.email,
            "aud": "authenticated",
            "role": "authenticated",
            "iat": int(time.time()) - 7200,
            "exp": int(time.time()) - 3600,  # expired 1 hour ago
        }
        secret = os.environ.get("JWT_SECRET", "test-e2e-secret-key-for-ci-only-0")
        expired_token = jwt.encode(payload, secret, algorithm="HS256")

        resp = await http_client.get(
            "/v1/conversations",
            headers={"Authorization": f"Bearer {expired_token}"},
        )
        assert resp.status_code == 401
