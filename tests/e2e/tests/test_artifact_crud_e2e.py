"""Artifact CRUD Operations E2E Tests.

Tests artifact create/read/delete operations (28 stories):
  - Happy path (AR01-AR08)
  - Read & list (AR09-AR15)
  - Delete (AR16-AR20)
  - Ownership & isolation (AR21-AR22)
  - Auth required (AR23-AR26)
  - Tier & error format (AR27-AR28)
"""

import re
import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, create_storyboard  # noqa: E402


@pytest.mark.artifacts
class TestArtifactCrudE2E:
    """Artifact CRUD end-to-end tests."""

    # -----------------------------------------------------------------------
    # Happy Path (AR01-AR08)
    # -----------------------------------------------------------------------

    async def test_ar01_create_storyboard_with_spec(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR01: POST /v1/artifacts/storyboards with spec -> 201, kind=storyboard."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/artifacts/storyboards",
            json={"spec": {"scenes": [{"duration": 5}]}},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Create storyboard failed: {resp.status_code} {resp.text}"
        )
        data = resp.json()
        assert data["kind"] == "storyboard"

    async def test_ar02_created_storyboard_has_status_ready(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR02: Created storyboard has status=ready (spec 8.16)."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        assert artifact["status"] == "ready"

    async def test_ar03_created_storyboard_has_source_upload(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR03: Created storyboard has source=upload."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        assert artifact["source"] == "upload"

    async def test_ar04_created_storyboard_metadata_empty(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR04: Created storyboard metadata={}."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        assert artifact["metadata"] == {}

    async def test_ar05_empty_spec_accepted(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR05: Empty spec {} -> 201 (valid JSON)."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers(), spec={})
        assert artifact["spec"] == {}

    async def test_ar06_complex_nested_spec_accepted(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR06: Complex nested spec -> 201, echoed back."""
        owner = seed_users.owner

        spec = {
            "scenes": [
                {
                    "id": 1,
                    "layers": [{"type": "video", "src": "clip.mp4"}],
                    "transitions": {"type": "fade", "duration": 0.5},
                }
            ],
            "settings": {"resolution": "1920x1080", "fps": 30},
        }
        artifact = await create_storyboard(http_client, owner.auth_headers(), spec=spec)
        assert artifact["spec"] == spec

    async def test_ar07_explicit_owner_urn_accepted(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR07: Explicit owner URN -> 201, matches."""
        owner = seed_users.owner

        owner_urn = f"framecast:user:{owner.user_id}"
        artifact = await create_storyboard(
            http_client, owner.auth_headers(), owner=owner_urn
        )
        assert artifact["owner"] == owner_urn

    async def test_ar08_invalid_owner_urn_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR08: Invalid owner URN -> 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/artifacts/storyboards",
            json={"spec": {}, "owner": "not-a-urn"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for invalid URN, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Read & List (AR09-AR15)
    # -----------------------------------------------------------------------

    async def test_ar09_create_then_get_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR09: Create then GET by ID -> same data."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        artifact_id = artifact["id"]

        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        fetched = resp.json()
        assert fetched["id"] == artifact_id
        assert fetched["kind"] == "storyboard"

    async def test_ar10_get_response_has_all_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR10: GET response has all expected fields."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        artifact_id = artifact["id"]

        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        data = resp.json()

        expected_fields = [
            "id",
            "owner",
            "created_by",
            "kind",
            "status",
            "source",
            "spec",
            "metadata",
            "created_at",
            "updated_at",
        ]
        for field in expected_fields:
            assert field in data, f"Missing field: {field}"

    async def test_ar11_artifact_id_is_valid_uuid(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR11: Artifact ID is valid UUID."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        uuid.UUID(artifact["id"])  # Raises if invalid

    async def test_ar12_timestamps_are_iso8601(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR12: Timestamps are ISO 8601."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        iso_pattern = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}")
        assert iso_pattern.match(artifact["created_at"]), (
            f"created_at not ISO 8601: {artifact['created_at']}"
        )
        assert iso_pattern.match(artifact["updated_at"]), (
            f"updated_at not ISO 8601: {artifact['updated_at']}"
        )

    async def test_ar13_create_three_list_returns_three(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR13: Create 3, list -> 3 items."""
        owner = seed_users.owner

        for _ in range(3):
            await create_storyboard(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifacts = resp.json()
        assert len(artifacts) >= 3

    async def test_ar14_list_newest_first(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR14: Create A then B; list -> B first (newest)."""
        owner = seed_users.owner

        a = await create_storyboard(
            http_client, owner.auth_headers(), spec={"order": "first"}
        )
        b = await create_storyboard(
            http_client, owner.auth_headers(), spec={"order": "second"}
        )

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifacts = resp.json()
        ids = [art["id"] for art in artifacts]
        assert ids.index(b["id"]) < ids.index(a["id"]), (
            "Newest artifact should be first"
        )

    async def test_ar15_fresh_user_lists_empty(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR15: Fresh user (invitee) lists -> []."""
        invitee = seed_users.invitee

        resp = await http_client.get("/v1/artifacts", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json() == []

    # -----------------------------------------------------------------------
    # Delete (AR16-AR20)
    # -----------------------------------------------------------------------

    async def test_ar16_delete_returns_204(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR16: DELETE -> 204."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        resp = await http_client.delete(
            f"/v1/artifacts/{artifact['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_ar17_delete_then_get_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR17: Delete then GET -> 404."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        artifact_id = artifact["id"]

        await http_client.delete(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_ar18_delete_one_of_three_leaves_two(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR18: Create 3, delete 1, list -> 2."""
        owner = seed_users.owner

        artifacts = []
        for _ in range(3):
            a = await create_storyboard(http_client, owner.auth_headers())
            artifacts.append(a)

        # Delete first
        await http_client.delete(
            f"/v1/artifacts/{artifacts[0]['id']}", headers=owner.auth_headers()
        )

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        remaining_ids = {a["id"] for a in resp.json()}
        assert artifacts[0]["id"] not in remaining_ids
        assert artifacts[1]["id"] in remaining_ids
        assert artifacts[2]["id"] in remaining_ids

    async def test_ar19_delete_same_id_twice(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR19: Delete same ID twice -> second 404."""
        owner = seed_users.owner

        artifact = await create_storyboard(http_client, owner.auth_headers())
        artifact_id = artifact["id"]

        resp = await http_client.delete(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        resp = await http_client.delete(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_ar20_delete_nonexistent_uuid(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR20: DELETE nonexistent UUID -> 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.delete(
            f"/v1/artifacts/{fake_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    # -----------------------------------------------------------------------
    # Ownership & Isolation (AR21-AR22)
    # -----------------------------------------------------------------------

    async def test_ar21_invitee_cannot_get_owners_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR21: Owner creates, invitee GETs -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        artifact = await create_storyboard(http_client, owner.auth_headers())
        resp = await http_client.get(
            f"/v1/artifacts/{artifact['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 404

    async def test_ar22_invitee_cannot_delete_owners_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR22: Owner creates, invitee DELETEs -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        artifact = await create_storyboard(http_client, owner.auth_headers())
        resp = await http_client.delete(
            f"/v1/artifacts/{artifact['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 404

    # -----------------------------------------------------------------------
    # Auth Required (AR23-AR26)
    # -----------------------------------------------------------------------

    async def test_ar23_create_storyboard_no_auth(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR23: POST /v1/artifacts/storyboards no auth -> 401."""
        resp = await http_client.post(
            "/v1/artifacts/storyboards",
            json={"spec": {}},
        )
        assert resp.status_code == 401

    async def test_ar24_list_artifacts_no_auth(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR24: GET /v1/artifacts no auth -> 401."""
        resp = await http_client.get("/v1/artifacts")
        assert resp.status_code == 401

    async def test_ar25_get_artifact_no_auth(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR25: GET /v1/artifacts/{id} no auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.get(f"/v1/artifacts/{fake_id}")
        assert resp.status_code == 401

    async def test_ar26_delete_artifact_no_auth(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR26: DELETE /v1/artifacts/{id} no auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.delete(f"/v1/artifacts/{fake_id}")
        assert resp.status_code == 401

    # -----------------------------------------------------------------------
    # Tier & Error Format (AR27-AR28)
    # -----------------------------------------------------------------------

    async def test_ar27_starter_can_create_storyboard(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR27: Starter user creates storyboard -> 201."""
        invitee = seed_users.invitee

        artifact = await create_storyboard(http_client, invitee.auth_headers())
        assert artifact["kind"] == "storyboard"

    async def test_ar28_error_format_consistency(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AR28: Trigger 400, verify error format {"error": {"code", "message"}}."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/artifacts/storyboards",
            json={"spec": {}, "owner": "not-a-urn"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400
        body = resp.json()
        assert "error" in body, f"Missing 'error' key in: {body}"
        assert "code" in body["error"], f"Missing 'code' in error: {body}"
        assert "message" in body["error"], f"Missing 'message' in error: {body}"
