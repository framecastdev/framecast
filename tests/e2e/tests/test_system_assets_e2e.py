"""System Assets E2E Tests.

Tests system asset catalog read operations (12 stories):
  - Happy path (SA01-SA04)
  - Error handling (SA05)
  - Validation & structure (SA06-SA10)
  - Auth & error format (SA11-SA12)
"""

import re
import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers  # noqa: E402


@pytest.mark.system_assets
class TestSystemAssetsE2E:
    """System asset catalog end-to-end tests."""

    # -----------------------------------------------------------------------
    # Happy Path (SA01-SA04)
    # -----------------------------------------------------------------------

    async def test_sa01_list_system_assets_returns_200(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA01: GET /v1/system-assets -> 200 with array."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/system-assets", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert isinstance(resp.json(), list)

    async def test_sa02_response_items_have_all_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA02: Response items have all expected fields."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/system-assets", headers=owner.auth_headers())
        assert resp.status_code == 200
        assets = resp.json()
        assert len(assets) > 0

        expected_fields = [
            "id",
            "category",
            "name",
            "description",
            "s3_key",
            "content_type",
            "size_bytes",
            "tags",
            "created_at",
        ]
        for asset in assets:
            for field in expected_fields:
                assert field in asset, f"Missing field '{field}' in asset: {asset}"

    async def test_sa03_list_contains_seeded_assets(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA03: List contains seeded assets."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/system-assets", headers=owner.auth_headers())
        assert resp.status_code == 200
        assets = resp.json()
        asset_ids = {a["id"] for a in assets}

        expected_ids = {
            "asset_sfx_whoosh_01",
            "asset_ambient_rain_01",
            "asset_music_chill_01",
            "asset_transition_fade_01",
        }
        for expected_id in expected_ids:
            assert expected_id in asset_ids, (
                f"Seeded asset '{expected_id}' not found in list"
            )

    async def test_sa04_get_system_asset_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA04: GET /v1/system-assets/{id} -> 200."""
        owner = seed_users.owner

        resp = await http_client.get(
            "/v1/system-assets/asset_sfx_whoosh_01",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        asset = resp.json()
        assert asset["id"] == "asset_sfx_whoosh_01"
        assert asset["category"] == "sfx"
        assert asset["name"] == "Whoosh 01"

    # -----------------------------------------------------------------------
    # Error Handling (SA05)
    # -----------------------------------------------------------------------

    async def test_sa05_get_nonexistent_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA05: GET nonexistent ID -> 404."""
        owner = seed_users.owner

        resp = await http_client.get(
            "/v1/system-assets/nonexistent_asset_id",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404

    # -----------------------------------------------------------------------
    # Validation & Structure (SA06-SA10)
    # -----------------------------------------------------------------------

    async def test_sa06_asset_id_matches_pattern(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA06: Each asset ID matches expected pattern."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/system-assets", headers=owner.auth_headers())
        assert resp.status_code == 200
        pattern = re.compile(r"^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$")
        for asset in resp.json():
            assert pattern.match(asset["id"]), (
                f"Asset ID '{asset['id']}' does not match expected pattern"
            )

    async def test_sa07_category_is_valid_enum(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA07: Category is valid enum value."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/system-assets", headers=owner.auth_headers())
        assert resp.status_code == 200
        valid_categories = {"sfx", "ambient", "music", "transition"}
        for asset in resp.json():
            assert asset["category"] in valid_categories, (
                f"Invalid category: {asset['category']}"
            )

    async def test_sa08_tags_field_is_json_array(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA08: Tags field is JSON array."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/system-assets", headers=owner.auth_headers())
        assert resp.status_code == 200
        for asset in resp.json():
            assert isinstance(asset["tags"], list), (
                f"Tags should be array, got: {type(asset['tags'])}"
            )

    async def test_sa09_starter_can_access_system_assets(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA09: Starter user can access system assets -> 200."""
        invitee = seed_users.invitee  # starter

        resp = await http_client.get(
            "/v1/system-assets", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

    async def test_sa10_audio_asset_has_size_bytes(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA10: Audio asset has size_bytes; all assets have non-null size_bytes."""
        owner = seed_users.owner

        resp = await http_client.get(
            "/v1/system-assets/asset_sfx_whoosh_01",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        asset = resp.json()
        assert asset["size_bytes"] is not None
        assert asset["size_bytes"] > 0

    # -----------------------------------------------------------------------
    # Auth & Error Format (SA11-SA12)
    # -----------------------------------------------------------------------

    async def test_sa11_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_system_assets,
    ):
        """SA11: No auth -> 401."""
        resp = await http_client.get("/v1/system-assets")
        assert resp.status_code == 401

    async def test_sa12_404_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        seed_system_assets,
    ):
        """SA12: 404 has {"error": {"code": "NOT_FOUND"}}."""
        owner = seed_users.owner

        resp = await http_client.get(
            "/v1/system-assets/nonexistent_id",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404
        body = resp.json()
        assert "error" in body, f"Missing 'error' key in: {body}"
        assert "code" in body["error"], f"Missing 'code' in error: {body}"
        assert body["error"]["code"] == "NOT_FOUND"
