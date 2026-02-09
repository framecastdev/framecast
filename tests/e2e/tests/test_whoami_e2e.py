"""Whoami E2E Tests.

Tests GET /v1/auth/whoami endpoint (10 stories):
  - JWT auth returns user context (WH01-WH02)
  - API key auth returns user context (WH03-WH04)
  - User tier details (WH05-WH06)
  - Error cases: no auth, expired JWT, revoked key (WH07-WH09)
  - Response shape completeness (WH10)
"""

import os
import sys
import time
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import jwt  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers  # noqa: E402


@pytest.mark.auth
class TestWhoamiE2E:
    """Whoami endpoint end-to-end tests."""

    # -----------------------------------------------------------------------
    # Helper
    # -----------------------------------------------------------------------

    async def _create_api_key(
        self,
        http_client: httpx.AsyncClient,
        headers: dict[str, str],
        name: str = "Whoami Test Key",
    ) -> tuple[str, str]:
        """Create an API key and return (key_id, raw_key)."""
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": name, "scopes": ["*"]},
            headers=headers,
        )
        assert resp.status_code == 201
        data = resp.json()
        return data["api_key"]["id"], data["raw_key"]

    # -----------------------------------------------------------------------
    # JWT Auth (WH01-WH02)
    # -----------------------------------------------------------------------

    async def test_wh01_jwt_returns_user_context(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH01: JWT auth -> 200, response has user.id, user.email, user.tier."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/auth/whoami", headers=owner.auth_headers())
        assert resp.status_code == 200
        data = resp.json()
        user = data["user"]
        assert user["id"] == owner.user_id
        assert user["email"] == owner.email
        assert user["tier"] == owner.tier

    async def test_wh02_jwt_auth_method_is_jwt(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH02: JWT auth -> auth_method = "jwt"."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/auth/whoami", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["auth_method"] == "jwt"

    # -----------------------------------------------------------------------
    # API Key Auth (WH03-WH04)
    # -----------------------------------------------------------------------

    async def test_wh03_api_key_returns_user_context(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH03: API key auth -> 200, has user.id, api_key.name."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/auth/whoami",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["user"]["id"] == owner.user_id
        assert data["api_key"]["name"] == "Whoami Test Key"

    async def test_wh04_api_key_auth_method_is_api_key(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH04: API key auth -> auth_method = "api_key"."""
        owner = seed_users.owner
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/auth/whoami",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200
        assert resp.json()["auth_method"] == "api_key"

    # -----------------------------------------------------------------------
    # Tier Details (WH05-WH06)
    # -----------------------------------------------------------------------

    async def test_wh05_creator_has_tier_creator(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH05: Creator user -> tier = "creator", upgraded_at is set."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/auth/whoami", headers=owner.auth_headers())
        assert resp.status_code == 200
        user = resp.json()["user"]
        assert user["tier"] == "creator"
        assert user["upgraded_at"] is not None

    async def test_wh06_starter_has_tier_starter(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH06: Starter user -> tier = "starter", upgraded_at is null."""
        invitee = seed_users.invitee

        resp = await http_client.get("/v1/auth/whoami", headers=invitee.auth_headers())
        assert resp.status_code == 200
        user = resp.json()["user"]
        assert user["tier"] == "starter"
        assert user["upgraded_at"] is None

    # -----------------------------------------------------------------------
    # Error Cases (WH07-WH09)
    # -----------------------------------------------------------------------

    async def test_wh07_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH07: No auth header -> 401."""
        resp = await http_client.get("/v1/auth/whoami")
        assert resp.status_code == 401

    async def test_wh08_expired_jwt_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH08: Expired JWT -> 401."""
        owner = seed_users.owner

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
            "/v1/auth/whoami",
            headers={"Authorization": f"Bearer {expired_token}"},
        )
        assert resp.status_code == 401

    async def test_wh09_revoked_api_key_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH09: Revoked API key -> 401."""
        owner = seed_users.owner
        key_id, raw_key = await self._create_api_key(http_client, owner.auth_headers())

        # Revoke the key
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Use revoked key
        resp = await http_client.get(
            "/v1/auth/whoami",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 401

    # -----------------------------------------------------------------------
    # Response Shape (WH10)
    # -----------------------------------------------------------------------

    async def test_wh10_response_has_all_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """WH10: Response has auth_method, user (with all fields), and api_key (nullable)."""
        owner = seed_users.owner

        # JWT: api_key should be absent (skip_serializing_if = None)
        resp = await http_client.get("/v1/auth/whoami", headers=owner.auth_headers())
        assert resp.status_code == 200
        data = resp.json()
        assert "auth_method" in data
        assert "user" in data
        assert "api_key" not in data  # skipped when None

        # Verify user has expected fields
        user = data["user"]
        expected_fields = {
            "id",
            "email",
            "name",
            "avatar_url",
            "tier",
            "credits",
            "ephemeral_storage_bytes",
            "upgraded_at",
            "created_at",
            "updated_at",
        }
        assert expected_fields.issubset(user.keys()), (
            f"Missing user fields: {expected_fields - user.keys()}"
        )

        # API key: api_key should be present with expected fields
        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        resp = await http_client.get(
            "/v1/auth/whoami",
            headers={"Authorization": f"Bearer {raw_key}"},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert "api_key" in data
        api_key = data["api_key"]
        expected_key_fields = {
            "id",
            "owner",
            "name",
            "key_prefix",
            "scopes",
            "expires_at",
        }
        assert expected_key_fields.issubset(api_key.keys()), (
            f"Missing api_key fields: {expected_key_fields - api_key.keys()}"
        )
