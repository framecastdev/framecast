"""API Key Operations E2E Tests.

Tests API key CRUD, scope validation, and tier restrictions (26 stories):
  - Happy path (AK1-AK10)
  - Scope validation by tier (AK11-AK17)
  - Owner URN validation (AK18-AK20)
  - Revocation & lifecycle (AK21-AK24)
  - Edge cases (AK25-AK26)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402


@pytest.mark.auth
class TestApiKeysE2E:
    """API key operations end-to-end tests."""

    # -----------------------------------------------------------------------
    # Happy Path (AK1-AK10)
    # -----------------------------------------------------------------------

    async def test_ak1_create_personal_api_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK1: POST /v1/auth/keys -> 201, raw_key returned."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Test Key", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Create API key failed: {resp.status_code} {resp.text}"
        )
        data = resp.json()
        assert "raw_key" in data, "Raw key should be in creation response"
        assert "api_key" in data
        assert "id" in data["api_key"]

    async def test_ak2_create_api_key_with_custom_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK2: POST with name="My CI Key" -> name persisted."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "My CI Key", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        assert resp.json()["api_key"]["name"] == "My CI Key"

    async def test_ak3_create_api_key_with_specific_scopes(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK3: POST with scopes=["generate","jobs:read"] -> scopes persisted."""
        owner = seed_users.owner

        scopes = ["generate", "jobs:read"]
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Scoped Key", "scopes": scopes},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        returned_scopes = resp.json()["api_key"].get("scopes", [])
        assert set(returned_scopes) == set(scopes)

    async def test_ak4_create_api_key_with_expiration(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK4: POST with expires_at in future -> expires_at persisted."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Expiring Key",
                "scopes": ["generate"],
                "expires_at": "2099-12-31T23:59:59Z",
            },
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        assert resp.json()["api_key"].get("expires_at") is not None

    async def test_ak5_list_api_keys(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK5: GET /v1/auth/keys -> returns all user's keys, key_hash NOT exposed."""
        owner = seed_users.owner

        # Create a key first
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "List Test Key", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201

        # List keys
        resp = await http_client.get("/v1/auth/keys", headers=owner.auth_headers())
        assert resp.status_code == 200
        keys = resp.json()
        assert isinstance(keys, list)
        assert len(keys) >= 1

        for k in keys:
            assert "key_hash" not in k, "key_hash should not be exposed"
            assert "raw_key" not in k, "raw_key should not be in list response"

    async def test_ak6_get_api_key_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK6: GET /v1/auth/keys/:id -> returns key details, no hash."""
        owner = seed_users.owner

        # Create
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Get By ID Key", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_id = resp.json()["api_key"]["id"]

        # Get
        resp = await http_client.get(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        key = resp.json()
        assert key["id"] == key_id
        assert "key_hash" not in key

    async def test_ak7_update_api_key_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK7: PATCH /v1/auth/keys/:id with new name -> 200."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Original Name", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_id = resp.json()["api_key"]["id"]

        resp = await http_client.patch(
            f"/v1/auth/keys/{key_id}",
            json={"name": "Updated Name"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Update key name failed: {resp.status_code} {resp.text}"
        )
        assert resp.json()["name"] == "Updated Name"

    async def test_ak8_revoke_api_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK8: DELETE /v1/auth/keys/:id -> 204, key immediately invalid."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Revoke Me", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_id = resp.json()["api_key"]["id"]

        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_ak9_raw_key_only_visible_on_creation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK9: After creation, GET key -> no raw_key field."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Visible Once Key", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        creation_data = resp.json()
        key_id = creation_data["api_key"]["id"]
        # Creation response should have raw_key at top level
        assert "raw_key" in creation_data

        # Subsequent GET should NOT have raw_key
        resp = await http_client.get(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert "raw_key" not in resp.json()

    async def test_ak10_creator_creates_team_scoped_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """AK10: Creator POST with owner=framecast:team:{team_id} -> 201."""
        owner = seed_users.owner

        # Create team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Create team-scoped key
        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Team Key",
                "scopes": ["generate"],
                "owner": f"framecast:team:{team_id}",
            },
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Create team-scoped key failed: {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Scope Validation by Tier (AK11-AK17)
    # -----------------------------------------------------------------------

    async def test_ak11_starter_allowed_scopes(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK11: Starter creates key with each allowed scope -> 201."""
        invitee = seed_users.invitee  # starter

        allowed = ["generate", "jobs:read", "jobs:write", "assets:read", "assets:write"]
        for scope in allowed:
            resp = await http_client.post(
                "/v1/auth/keys",
                json={"name": f"Starter {scope}", "scopes": [scope]},
                headers=invitee.auth_headers(),
            )
            assert resp.status_code == 201, (
                f"Starter should be allowed scope '{scope}', got {resp.status_code} {resp.text}"
            )

    async def test_ak12_starter_denied_team_read_scope(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK12: Starter POST with scopes=["team:read"] -> 400."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Denied Key", "scopes": ["team:read"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for starter team:read, got {resp.status_code} {resp.text}"
        )

    async def test_ak13_starter_denied_team_admin_scope(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK13: Starter POST with scopes=["team:admin"] -> 400."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Denied Key", "scopes": ["team:admin"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for starter team:admin, got {resp.status_code} {resp.text}"
        )

    async def test_ak14_starter_denied_wildcard_scope(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK14: Starter POST with scopes=["*"] -> 400."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Denied Key", "scopes": ["*"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for starter wildcard, got {resp.status_code} {resp.text}"
        )

    async def test_ak15_starter_denied_projects_read_scope(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK15: Starter POST with scopes=["projects:read"] -> 400."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Denied Key", "scopes": ["projects:read"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for starter projects:read, got {resp.status_code} {resp.text}"
        )

    async def test_ak16_creator_can_use_all_scopes(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK16: Creator POST with scopes=["*"] -> 201."""
        owner = seed_users.owner  # creator

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Wildcard Key", "scopes": ["*"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Creator should allow wildcard scope, got {resp.status_code} {resp.text}"
        )

    async def test_ak17_starter_default_scopes_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK17: Starter POST with no scopes field -> 400 (default is ["*"])."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "No Scopes Key"},
            headers=invitee.auth_headers(),
        )
        # If default scope is * and starter can't use *, this should fail
        assert resp.status_code in [400, 403, 422], (
            f"Expected rejection for starter default scopes, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Owner URN Validation (AK18-AK20)
    # -----------------------------------------------------------------------

    async def test_ak18_starter_cannot_create_team_scoped_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK18: Starter POST with owner=framecast:team:{id} -> 400 (INV-A6)."""
        invitee = seed_users.invitee

        fake_team_id = str(uuid.uuid4())
        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Team Key Attempt",
                "scopes": ["generate"],
                "owner": f"framecast:team:{fake_team_id}",
            },
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for starter team-scoped key, got {resp.status_code} {resp.text}"
        )

    async def test_ak19_creator_can_create_team_scoped_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """AK19: Creator in team -> POST with team URN -> 201."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Team Scoped",
                "scopes": ["generate"],
                "owner": f"framecast:team:{team_id}",
            },
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201

    async def test_ak20_creator_cannot_create_key_for_other_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK20: Creator POST with other team's URN -> 403."""
        owner = seed_users.owner

        fake_team_id = str(uuid.uuid4())
        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Other Team Key",
                "scopes": ["generate"],
                "owner": f"framecast:team:{fake_team_id}",
            },
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 403, 404], (
            f"Expected 400/403/404 for non-member team key, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Revocation & Lifecycle (AK21-AK24)
    # -----------------------------------------------------------------------

    async def test_ak21_revoked_key_cannot_be_used_for_auth(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK21: Create key -> revoke -> use key in request -> 401."""
        owner = seed_users.owner

        # Create key
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Revoke Auth Test", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_data = resp.json()
        key_id = key_data["api_key"]["id"]
        raw_key = key_data.get("raw_key", "")

        # Revoke
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Try to use revoked key
        if raw_key:
            resp = await http_client.get(
                "/v1/account",
                headers={"Authorization": f"Bearer {raw_key}"},
            )
            assert resp.status_code == 401, (
                f"Expected 401 for revoked key, got {resp.status_code}"
            )

    async def test_ak22_revoke_already_revoked_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK22: DELETE on already-revoked key -> 400/404."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Double Revoke", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_id = resp.json()["api_key"]["id"]

        # First revoke
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Second revoke
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code in [400, 404, 409], (
            f"Expected 400/404/409 for double revoke, got {resp.status_code} {resp.text}"
        )

    async def test_ak23_update_revoked_key_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK23: PATCH on revoked key -> 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Revoke Then Update", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_id = resp.json()["api_key"]["id"]

        # Revoke
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Try update
        resp = await http_client.patch(
            f"/v1/auth/keys/{key_id}",
            json={"name": "Updated After Revoke"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 404, 409], (
            f"Expected 400/404/409 for updating revoked key, got {resp.status_code} {resp.text}"
        )

    async def test_ak24_expired_key_cannot_be_used(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK24: Create key with expires_at=past -> use -> 401."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Expired Key",
                "scopes": ["generate"],
                "expires_at": "2020-01-01T00:00:00Z",
            },
            headers=owner.auth_headers(),
        )
        # Server may reject past expiry at creation time (400) or accept it
        if resp.status_code == 201:
            raw_key = resp.json().get("raw_key", "")
            if raw_key:
                resp = await http_client.get(
                    "/v1/account",
                    headers={"Authorization": f"Bearer {raw_key}"},
                )
                assert resp.status_code == 401
        else:
            assert resp.status_code == 400

    # -----------------------------------------------------------------------
    # Edge Cases (AK25-AK26)
    # -----------------------------------------------------------------------

    async def test_ak25_create_key_with_invalid_scope(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK25: POST with scopes=["nonexistent"] -> 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Invalid Scope", "scopes": ["nonexistent_scope"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for invalid scope, got {resp.status_code} {resp.text}"
        )

    async def test_ak26_get_another_users_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """AK26: GET /v1/auth/keys/{other-user-key-id} -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates key
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Owner Key", "scopes": ["generate"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        key_id = resp.json()["api_key"]["id"]

        # Invitee tries to access owner's key
        resp = await http_client.get(
            f"/v1/auth/keys/{key_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404], (
            f"Expected 403/404 for accessing other user's key, got {resp.status_code}"
        )

    # -----------------------------------------------------------------------
    # Validation: Parametrized Tests
    # -----------------------------------------------------------------------

    @pytest.mark.parametrize(
        "scopes",
        [
            ["generate"],
            ["jobs:read", "jobs:write"],
            ["generate", "assets:read", "assets:write"],
        ],
        ids=["single-scope", "jobs-pair", "generate-plus-assets"],
    )
    async def test_starter_valid_scopes_always_succeed(
        self,
        scopes: list[str],
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """Any combination of starter-allowed scopes succeeds."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Parametrize Test", "scopes": scopes},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Starter scopes {scopes} should succeed, got {resp.status_code} {resp.text}"
        )

    @pytest.mark.parametrize(
        "scopes",
        [
            ["invalid_scope_xyz"],
            ["badscope", "another_bad"],
            ["nonexistent"],
        ],
        ids=["single-invalid", "two-invalid", "nonexistent"],
    )
    async def test_invalid_scopes_always_rejected(
        self,
        scopes: list[str],
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """Invalid scope names always return 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Invalid Scope Test", "scopes": scopes},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for invalid scopes {scopes}, got {resp.status_code}"
        )
