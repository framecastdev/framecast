"""Conversation Lifecycle E2E Tests.

Tests conversation create/read/update/delete operations (24 stories):
  - Happy path (CV01-CV07)
  - List & filter (CV08-CV11)
  - Update (CV12-CV14)
  - Delete (CV15-CV16)
  - Ownership (CV17-CV19)
  - Auth & validation (CV20-CV24)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, create_conversation  # noqa: E402


@pytest.mark.conversations
class TestConversationLifecycleE2E:
    """Conversation lifecycle end-to-end tests."""

    # -----------------------------------------------------------------------
    # Happy Path (CV01-CV07)
    # -----------------------------------------------------------------------

    async def test_cv01_create_with_model_only(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV01: POST with model only -> 201."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/conversations",
            json={"model": "test-model"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Create conversation failed: {resp.status_code} {resp.text}"
        )
        data = resp.json()
        assert data["model"] == "test-model"

    async def test_cv02_create_with_all_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV02: POST with model+title+system_prompt -> all persisted."""
        owner = seed_users.owner

        conv = await create_conversation(
            http_client,
            owner.auth_headers(),
            model="claude-test",
            title="My Chat",
            system_prompt="You are a helpful assistant.",
        )
        assert conv["model"] == "claude-test"
        assert conv["title"] == "My Chat"
        assert conv["system_prompt"] == "You are a helpful assistant."

    async def test_cv03_created_status_is_active(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV03: Created status=active."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        assert conv["status"] == "active"

    async def test_cv04_created_message_count_zero(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV04: Created message_count=0."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        assert conv["message_count"] == 0

    async def test_cv05_created_last_message_at_null(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV05: Created last_message_at=null."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        assert conv["last_message_at"] is None

    async def test_cv06_create_then_get_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV06: Create then GET by ID -> same data."""
        owner = seed_users.owner

        conv = await create_conversation(
            http_client, owner.auth_headers(), title="Test Get"
        )
        conv_id = conv["id"]

        resp = await http_client.get(
            f"/v1/conversations/{conv_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        fetched = resp.json()
        assert fetched["id"] == conv_id
        assert fetched["title"] == "Test Get"

    async def test_cv07_get_response_has_all_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV07: GET response has all fields."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        conv_id = conv["id"]

        resp = await http_client.get(
            f"/v1/conversations/{conv_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        data = resp.json()

        expected_fields = [
            "id",
            "user_id",
            "title",
            "model",
            "system_prompt",
            "status",
            "message_count",
            "last_message_at",
            "created_at",
            "updated_at",
        ]
        for field in expected_fields:
            assert field in data, f"Missing field: {field}"

    # -----------------------------------------------------------------------
    # List & Filter (CV08-CV11)
    # -----------------------------------------------------------------------

    async def test_cv08_create_two_list_both(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV08: Create 2, list -> both present."""
        owner = seed_users.owner

        c1 = await create_conversation(http_client, owner.auth_headers())
        c2 = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/conversations", headers=owner.auth_headers())
        assert resp.status_code == 200
        conv_ids = {c["id"] for c in resp.json()}
        assert c1["id"] in conv_ids
        assert c2["id"] in conv_ids

    async def test_cv09_fresh_user_lists_empty(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV09: Fresh user lists -> []."""
        invitee = seed_users.invitee

        resp = await http_client.get(
            "/v1/conversations", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json() == []

    async def test_cv10_default_list_excludes_archived(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV10: Active + archived; default list -> only active."""
        owner = seed_users.owner

        active_conv = await create_conversation(
            http_client, owner.auth_headers(), title="Active One"
        )
        archived_conv = await create_conversation(
            http_client, owner.auth_headers(), title="To Archive"
        )

        # Archive the second
        resp = await http_client.patch(
            f"/v1/conversations/{archived_conv['id']}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Default list
        resp = await http_client.get("/v1/conversations", headers=owner.auth_headers())
        assert resp.status_code == 200
        conv_ids = {c["id"] for c in resp.json()}
        assert active_conv["id"] in conv_ids
        assert archived_conv["id"] not in conv_ids

    async def test_cv11_status_filter_archived(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV11: ?status=archived -> only archived."""
        owner = seed_users.owner

        active_conv = await create_conversation(http_client, owner.auth_headers())
        archived_conv = await create_conversation(http_client, owner.auth_headers())

        # Archive one
        await http_client.patch(
            f"/v1/conversations/{archived_conv['id']}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )

        resp = await http_client.get(
            "/v1/conversations?status=archived", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        conv_ids = {c["id"] for c in resp.json()}
        assert archived_conv["id"] in conv_ids
        assert active_conv["id"] not in conv_ids

    # -----------------------------------------------------------------------
    # Update (CV12-CV14)
    # -----------------------------------------------------------------------

    async def test_cv12_update_title(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV12: PATCH title -> changed."""
        owner = seed_users.owner

        conv = await create_conversation(
            http_client, owner.auth_headers(), title="Original"
        )

        resp = await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"title": "Updated Title"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["title"] == "Updated Title"

    async def test_cv13_archive_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV13: PATCH status=archived -> archived."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "archived"

    async def test_cv14_unarchive_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV14: Archive then PATCH status=active -> active."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        # Archive
        await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )

        # Unarchive
        resp = await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"status": "active"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "active"

    # -----------------------------------------------------------------------
    # Delete (CV15-CV16)
    # -----------------------------------------------------------------------

    async def test_cv15_delete_returns_204(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV15: DELETE -> 204."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.delete(
            f"/v1/conversations/{conv['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_cv16_delete_then_get_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV16: DELETE then GET -> 404."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        conv_id = conv["id"]

        await http_client.delete(
            f"/v1/conversations/{conv_id}", headers=owner.auth_headers()
        )

        resp = await http_client.get(
            f"/v1/conversations/{conv_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    # -----------------------------------------------------------------------
    # Ownership (CV17-CV19)
    # -----------------------------------------------------------------------

    async def test_cv17_invitee_cannot_get_owners_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV17: Invitee GETs owner's conversation -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 404

    async def test_cv18_invitee_cannot_patch_owners_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV18: Invitee PATCHes owner's -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"title": "Hacked"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 404

    async def test_cv19_invitee_cannot_delete_owners_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV19: Invitee DELETEs owner's -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.delete(
            f"/v1/conversations/{conv['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 404

    # -----------------------------------------------------------------------
    # Auth & Validation (CV20-CV24)
    # -----------------------------------------------------------------------

    async def test_cv20_create_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV20: POST no auth -> 401."""
        resp = await http_client.post(
            "/v1/conversations",
            json={"model": "test-model"},
        )
        assert resp.status_code == 401

    async def test_cv21_create_missing_model_returns_422(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV21: POST missing model -> 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/conversations",
            json={},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for missing model, got {resp.status_code} {resp.text}"
        )

    async def test_cv22_model_101_chars_returns_422(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV22: Model 101 chars -> 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/conversations",
            json={"model": "a" * 101},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for model too long, got {resp.status_code} {resp.text}"
        )

    async def test_cv23_title_201_chars_returns_422(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV23: Title 201 chars -> 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/conversations",
            json={"model": "test-model", "title": "a" * 201},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for title too long, got {resp.status_code} {resp.text}"
        )

    async def test_cv24_system_prompt_10001_chars_returns_422(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """CV24: System prompt 10001 chars -> 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/conversations",
            json={"model": "test-model", "system_prompt": "a" * 10001},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for prompt too long, got {resp.status_code} {resp.text}"
        )
