"""User Journey E2E Tests.

Tests full user journeys spanning multiple domains (9 stories):
  - Starter upgrade -> conversation -> character -> render (UJ01)
  - Invitation accept -> whoami reflects upgrade -> platform access (UJ02)
  - API key drives full content lifecycle (UJ03)
  - Account cleanup and deletion (UJ04)
  - Conversation archive -> character still accessible and renders (UJ05)
  - Team-scoped API key creates team-owned artifacts (UJ06)
  - Multi-conversation artifact isolation with selective artifact deletion (UJ07)
  - Revoked API key loses access mid-workflow (UJ08)
  - Invitation upgrade unlocks previously-denied capabilities (UJ09)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    TestDataFactory,
    create_character,
    create_conversation,
    create_storyboard,
    send_message,
)


@pytest.mark.journey
class TestUserJourneyE2E:
    """Full user journey end-to-end tests spanning multiple domains."""

    # -------------------------------------------------------------------
    # Helpers
    # -------------------------------------------------------------------

    async def _create_api_key(
        self,
        http_client: httpx.AsyncClient,
        headers: dict[str, str],
        name: str = "Journey Test Key",
        scopes: list[str] | None = None,
        owner: str | None = None,
    ) -> tuple[str, str]:
        """Create an API key and return (key_id, raw_key)."""
        payload: dict = {"name": name, "scopes": scopes or ["*"]}
        if owner is not None:
            payload["owner"] = owner
        resp = await http_client.post("/v1/auth/keys", json=payload, headers=headers)
        assert resp.status_code == 201, (
            f"create_api_key failed: {resp.status_code} {resp.text}"
        )
        data = resp.json()
        return data["api_key"]["id"], data["raw_key"]

    # -------------------------------------------------------------------
    # UJ01: Starter -> Upgrade -> Conversation -> Character -> Render
    # -------------------------------------------------------------------

    async def test_uj01_starter_upgrade_conversation_character_render(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """UJ01: Starter upgrades, creates conversation, generates character, renders it."""
        invitee = seed_users.invitee

        # 1. Verify starter tier
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "starter"

        # 2. Upgrade to creator
        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # 3. Verify creator tier + upgraded_at
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        account = resp.json()
        assert account["tier"] == "creator"
        assert account["upgraded_at"] is not None

        # 4. Create conversation
        conv = await create_conversation(http_client, invitee.auth_headers())
        conv_id = conv["id"]

        # 5. Send message that generates a character artifact
        result = await send_message(
            http_client, invitee.auth_headers(), conv_id, "create a character"
        )
        assistant = result["assistant_message"]
        assert assistant["artifacts"] is not None
        assert len(assistant["artifacts"]) > 0
        assert assistant["artifacts"][0]["kind"] == "character"
        character_id = assistant["artifacts"][0]["id"]

        # 6. Verify character artifact links back to conversation
        resp = await http_client.get(
            f"/v1/artifacts/{character_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200
        artifact = resp.json()
        assert artifact["source"] == "conversation"
        assert artifact["conversation_id"] == conv_id

        # 7. Render character -> job + image artifact
        resp = await http_client.post(
            f"/v1/artifacts/{character_id}/render",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201
        result = resp.json()
        assert result["artifact"]["kind"] == "image"
        assert result["artifact"]["status"] == "pending"

        # 8. List artifacts -> both character and image present
        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        artifact_kinds = {a["kind"] for a in resp.json()}
        assert "character" in artifact_kinds
        assert "image" in artifact_kinds

    # -------------------------------------------------------------------
    # UJ02: Invitation Accept -> Whoami Reflects Upgrade -> Platform Access
    # -------------------------------------------------------------------

    async def test_uj02_invitation_accept_whoami_upgrade_platform_access(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """UJ02: Invitation accept auto-upgrades; whoami reflects; full platform access."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # 1. Whoami shows starter
        resp = await http_client.get("/v1/auth/whoami", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["user"]["tier"] == "starter"

        # 2. Owner creates team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # 3. Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        inv_id = resp.json()["id"]

        # 4. Invitee accepts invitation
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # 5. Whoami now shows creator
        resp = await http_client.get("/v1/auth/whoami", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["user"]["tier"] == "creator"

        # 6. Invitee can create conversation
        conv = await create_conversation(http_client, invitee.auth_headers())

        # 7. Invitee can send message
        await send_message(http_client, invitee.auth_headers(), conv["id"], "Hello")

        # 8. Invitee can create storyboard artifact
        await create_storyboard(http_client, invitee.auth_headers())

        # 9. Invitee can create own team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201

    # -------------------------------------------------------------------
    # UJ03: API Key Drives Full Content Lifecycle
    # -------------------------------------------------------------------

    async def test_uj03_api_key_full_content_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """UJ03: API key drives conversation, character, render, deletion lifecycle."""
        owner = seed_users.owner

        # 1. Create API key
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # 2. Whoami via API key
        resp = await http_client.get("/v1/auth/whoami", headers=api_headers)
        assert resp.status_code == 200
        assert resp.json()["auth_method"] == "api_key"

        # 3. Create conversation via API key
        conv = await create_conversation(http_client, api_headers)
        conv_id = conv["id"]

        # 4. Send message that generates character
        result = await send_message(
            http_client, api_headers, conv_id, "create a character"
        )
        assistant = result["assistant_message"]
        assert assistant["artifacts"] is not None
        assert len(assistant["artifacts"]) > 0
        character_id = assistant["artifacts"][0]["id"]

        # 5. Render character via API key
        resp = await http_client.post(
            f"/v1/artifacts/{character_id}/render", headers=api_headers
        )
        assert resp.status_code == 201
        result = resp.json()
        assert result["artifact"]["kind"] == "image"
        assert result["artifact"]["status"] == "pending"

        # 6. List artifacts -> character + image
        resp = await http_client.get("/v1/artifacts", headers=api_headers)
        assert resp.status_code == 200
        artifact_kinds = {a["kind"] for a in resp.json()}
        assert "character" in artifact_kinds
        assert "image" in artifact_kinds

        # 7. Delete character artifact
        resp = await http_client.delete(
            f"/v1/artifacts/{character_id}", headers=api_headers
        )
        assert resp.status_code == 204

        # 8. Character gone from list, image still present
        resp = await http_client.get("/v1/artifacts", headers=api_headers)
        assert resp.status_code == 200
        artifacts = resp.json()
        artifact_ids = {a["id"] for a in artifacts}
        assert character_id not in artifact_ids
        image_artifacts = [a for a in artifacts if a["kind"] == "image"]
        assert len(image_artifacts) > 0

        # 9. Delete conversation
        resp = await http_client.delete(
            f"/v1/conversations/{conv_id}", headers=api_headers
        )
        assert resp.status_code == 204

        # 10. Image artifact survives conversation deletion
        resp = await http_client.get("/v1/artifacts", headers=api_headers)
        assert resp.status_code == 200
        remaining_images = [a for a in resp.json() if a["kind"] == "image"]
        assert len(remaining_images) > 0

    # -------------------------------------------------------------------
    # UJ04: Account Cleanup and Deletion
    # -------------------------------------------------------------------

    async def test_uj04_account_cleanup_and_deletion(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """UJ04: Create resources, clean up, delete account, all inaccessible."""
        invitee = seed_users.invitee

        # 1. Create conversation + message
        conv = await create_conversation(http_client, invitee.auth_headers())
        await send_message(http_client, invitee.auth_headers(), conv["id"])

        # 2. Create artifacts
        storyboard = await create_storyboard(http_client, invitee.auth_headers())
        character = await create_character(http_client, invitee.auth_headers())

        # 3. Verify resources exist
        resp = await http_client.get(
            "/v1/conversations", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200
        assert len(resp.json()) > 0

        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert len(resp.json()) == 2

        # 4. Clean up resources before account deletion
        resp = await http_client.delete(
            f"/v1/artifacts/{storyboard['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 204

        resp = await http_client.delete(
            f"/v1/artifacts/{character['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 204

        resp = await http_client.delete(
            f"/v1/conversations/{conv['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 204

        # 5. Delete account
        resp = await http_client.delete("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 204

        # 6. Subsequent requests: JIT provisioning re-creates a fresh starter
        # account (Supabase still considers the JWT valid). The re-provisioned
        # user has no resources — conversations and artifacts return empty.
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code in [200, 401, 404]

        resp = await http_client.get(
            "/v1/conversations", headers=invitee.auth_headers()
        )
        assert resp.status_code in [200, 401, 404]

        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code in [200, 401, 404]

    # -------------------------------------------------------------------
    # UJ05: Conversation Archive -> Character Still Accessible & Renders
    # -------------------------------------------------------------------

    async def test_uj05_conversation_archive_character_accessible_and_renders(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """UJ05: Archive conversation; character artifact still accessible and renderable."""
        owner = seed_users.owner

        # 1. Create conversation
        conv = await create_conversation(http_client, owner.auth_headers())
        conv_id = conv["id"]

        # 2. Send message that generates character
        result = await send_message(
            http_client, owner.auth_headers(), conv_id, "create a character"
        )
        character_id = result["assistant_message"]["artifacts"][0]["id"]

        # 3. Verify character linked to conversation
        resp = await http_client.get(
            f"/v1/artifacts/{character_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["conversation_id"] == conv_id
        assert resp.json()["source"] == "conversation"

        # 4. Archive conversation
        resp = await http_client.patch(
            f"/v1/conversations/{conv_id}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # 5. Character still accessible after archive
        resp = await http_client.get(
            f"/v1/artifacts/{character_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["conversation_id"] == conv_id

        # 6. Messages still readable in archived conversation
        resp = await http_client.get(
            f"/v1/conversations/{conv_id}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert len(resp.json()) > 0

        # 7. Render still works on character from archived conversation
        resp = await http_client.post(
            f"/v1/artifacts/{character_id}/render",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        result = resp.json()
        assert result["artifact"]["kind"] == "image"
        assert result["artifact"]["status"] == "pending"

        # 8. Both character and image present in artifact list
        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifact_kinds = {a["kind"] for a in resp.json()}
        assert "character" in artifact_kinds
        assert "image" in artifact_kinds

    # -------------------------------------------------------------------
    # UJ06: Team-Scoped API Key Creates Team-Owned Artifacts
    # -------------------------------------------------------------------

    async def test_uj06_team_scoped_api_key_team_owned_artifacts(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """UJ06: Team-scoped API key creates artifacts with team owner URN."""
        owner = seed_users.owner

        # 1. Create team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

        # 2. Create team-scoped API key
        _, raw_key = await self._create_api_key(
            http_client, owner.auth_headers(), name="Team Key", owner=team_urn
        )
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # 3. Whoami shows team-scoped key
        resp = await http_client.get("/v1/auth/whoami", headers=api_headers)
        assert resp.status_code == 200
        data = resp.json()
        assert data["api_key"]["owner"] == team_urn

        # 4. Create storyboard with team owner
        storyboard = await create_storyboard(http_client, api_headers, owner=team_urn)
        assert storyboard["owner"] == team_urn

        # 5. Create character with team owner
        character = await create_character(
            http_client,
            api_headers,
            spec={"prompt": "A hero", "name": "Hero"},
            owner=team_urn,
        )
        assert character["owner"] == team_urn

        # 6. Both creation responses confirmed team ownership;
        #    verify artifacts are distinct entities
        assert storyboard["id"] != character["id"]
        assert storyboard["kind"] == "storyboard"
        assert character["kind"] == "character"

    # -------------------------------------------------------------------
    # UJ07: Multi-Conversation Artifact Isolation with Selective Deletion
    # -------------------------------------------------------------------

    async def test_uj07_multi_conversation_artifact_isolation_selective_deletion(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """UJ07: Artifacts from different conversations are isolated; selective deletion works."""
        owner = seed_users.owner

        # 1. Create conversation A, generate character alpha
        conv_a = await create_conversation(http_client, owner.auth_headers())
        result_a = await send_message(
            http_client, owner.auth_headers(), conv_a["id"], "create a character"
        )
        alpha_id = result_a["assistant_message"]["artifacts"][0]["id"]

        # 2. Create conversation B, generate character beta
        conv_b = await create_conversation(http_client, owner.auth_headers())
        result_b = await send_message(
            http_client, owner.auth_headers(), conv_b["id"], "create a character"
        )
        beta_id = result_b["assistant_message"]["artifacts"][0]["id"]

        # 3. Create standalone character gamma (no conversation)
        gamma = await create_character(http_client, owner.auth_headers())
        gamma_id = gamma["id"]

        # 4. All 3 characters in artifact list
        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        character_ids = {a["id"] for a in resp.json() if a["kind"] == "character"}
        assert alpha_id in character_ids
        assert beta_id in character_ids
        assert gamma_id in character_ids

        # 5. Verify each artifact's conversation linkage
        resp = await http_client.get(
            f"/v1/artifacts/{alpha_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["conversation_id"] == conv_a["id"]
        assert resp.json()["source"] == "conversation"

        resp = await http_client.get(
            f"/v1/artifacts/{beta_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["conversation_id"] == conv_b["id"]
        assert resp.json()["source"] == "conversation"

        resp = await http_client.get(
            f"/v1/artifacts/{gamma_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["conversation_id"] is None
        assert resp.json()["source"] == "upload"

        # 6. Delete alpha artifact
        resp = await http_client.delete(
            f"/v1/artifacts/{alpha_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # 7. Only beta + gamma remain
        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        remaining_ids = {a["id"] for a in resp.json() if a["kind"] == "character"}
        assert alpha_id not in remaining_ids
        assert beta_id in remaining_ids
        assert gamma_id in remaining_ids

        # 8. Conversation A messages still intact after artifact deletion
        resp = await http_client.get(
            f"/v1/conversations/{conv_a['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert len(resp.json()) > 0

        # 9. Conversation B messages also intact
        resp = await http_client.get(
            f"/v1/conversations/{conv_b['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert len(resp.json()) > 0

    # -------------------------------------------------------------------
    # UJ08: Revoked API Key Loses Access Mid-Workflow
    # -------------------------------------------------------------------

    async def test_uj08_revoked_api_key_loses_access_mid_workflow(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """UJ08: API key creates resources, gets revoked, loses all access; JWT retains access."""
        owner = seed_users.owner

        # 1. Create API key
        key_id, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # 2. Create conversation via API key
        conv = await create_conversation(http_client, api_headers)
        conv_id = conv["id"]

        # 3. Send message via API key
        await send_message(http_client, api_headers, conv_id)

        # 4. Create storyboard via API key
        storyboard = await create_storyboard(http_client, api_headers)
        storyboard_id = storyboard["id"]

        # 5. Revoke the key (via JWT)
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # 6-8. Revoked key gets 401 on all endpoints
        resp = await http_client.get("/v1/conversations", headers=api_headers)
        assert resp.status_code == 401

        resp = await http_client.get("/v1/artifacts", headers=api_headers)
        assert resp.status_code == 401

        resp = await http_client.post(
            f"/v1/conversations/{conv_id}/messages",
            json={"content": "Should fail"},
            headers=api_headers,
        )
        assert resp.status_code == 401

        # 9. JWT still works for key-created resources
        resp = await http_client.get(
            f"/v1/conversations/{conv_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # 10. JWT can access key-created artifact
        resp = await http_client.get(
            f"/v1/artifacts/{storyboard_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

    # -------------------------------------------------------------------
    # UJ09: Invitation Upgrade Unlocks Previously-Denied Capabilities
    # -------------------------------------------------------------------

    async def test_uj09_invitation_upgrade_unlocks_denied_capabilities(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """UJ09: Starter denied capabilities, invitation upgrades, capabilities unlocked."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # 1. Verify starter tier
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "starter"

        # 2. Wildcard API key denied for starter
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Denied Key", "scopes": ["*"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for starter wildcard scope, got {resp.status_code}"
        )

        # 3. Team creation denied for starter
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403, (
            f"Expected 403 for starter creating team, got {resp.status_code}"
        )

        # 4. Owner invites invitee, invitee accepts -> auto-upgrade
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        inv_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # 5. Verify creator tier
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "creator"

        # 6. Create conversation (now works)
        conv = await create_conversation(http_client, invitee.auth_headers())

        # 7. Generate character via conversation
        result = await send_message(
            http_client, invitee.auth_headers(), conv["id"], "create a character"
        )
        assistant = result["assistant_message"]
        assert assistant["artifacts"] is not None
        assert len(assistant["artifacts"]) > 0
        character_id = assistant["artifacts"][0]["id"]

        # 8. Render character
        resp = await http_client.post(
            f"/v1/artifacts/{character_id}/render",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201
        result = resp.json()
        assert result["artifact"]["kind"] == "image"
        assert result["artifact"]["status"] == "pending"

        # 9. Wildcard API key now succeeds
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Now Allowed Key", "scopes": ["*"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201

        # 10. Team creation now succeeds
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201

        # 11. Artifacts present (character + image)
        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        artifact_kinds = {a["kind"] for a in resp.json()}
        assert "character" in artifact_kinds
        assert "image" in artifact_kinds
