"""Generation Access Control E2E Tests.

Tests permissions and RBAC for generations (20 stories):
  - Owner-only access (GA-01 through GA-05)
  - Cross-user isolation (GA-06 through GA-10)
  - Team-scoped generations (GA-11 through GA-15)
  - Auth method variations (GA-16 through GA-20)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    TestDataFactory,
    complete_generation,
    create_ephemeral_generation,
)


@pytest.mark.generation_access
class TestGenerationAccessControlE2E:
    """Generation access control end-to-end tests."""

    async def _create_api_key(
        self,
        http_client: httpx.AsyncClient,
        headers: dict[str, str],
        name: str = "Access Test Key",
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
    # Owner-Only Access (GA-01 through GA-05)
    # -------------------------------------------------------------------

    async def test_ga01_owner_can_get_own_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-01: Owner can GET their own generation."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        resp = await http_client.get(
            f"/v1/generations/{gen['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["id"] == gen["id"]

    async def test_ga02_owner_can_cancel_own_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-02: Owner can cancel their own generation."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        resp = await http_client.post(
            f"/v1/generations/{gen['id']}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

    async def test_ga03_owner_can_delete_own_completed_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-03: Owner can delete their own completed generation."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        await complete_generation(http_client, gen["id"])

        resp = await http_client.delete(
            f"/v1/generations/{gen['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_ga04_owner_can_clone_own_completed_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-04: Owner can clone their own completed generation."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        await complete_generation(http_client, gen["id"])

        resp = await http_client.post(
            f"/v1/generations/{gen['id']}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201

    async def test_ga05_owner_can_list_own_generations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-05: Owner can list their own generations."""
        owner = seed_users.owner

        await create_ephemeral_generation(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/generations", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert len(resp.json()) >= 1

    # -------------------------------------------------------------------
    # Cross-User Isolation (GA-06 through GA-10)
    # -------------------------------------------------------------------

    async def test_ga06_other_user_cannot_get_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-06: Another user cannot GET someone else's generation."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        resp = await http_client.get(
            f"/v1/generations/{gen['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ga07_other_user_cannot_cancel_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-07: Another user cannot cancel someone else's generation."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        resp = await http_client.post(
            f"/v1/generations/{gen['id']}/cancel", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ga08_other_user_cannot_delete_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-08: Another user cannot delete someone else's generation."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        await complete_generation(http_client, gen["id"])

        resp = await http_client.delete(
            f"/v1/generations/{gen['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ga09_other_user_generations_not_in_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-09: Another user's generations don't appear in your list."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/generations", headers=invitee.auth_headers())
        assert resp.status_code == 200
        generation_ids = {g["id"] for g in resp.json()}
        assert gen["id"] not in generation_ids

    async def test_ga10_other_user_cannot_clone_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-10: Another user cannot clone someone else's generation."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        await complete_generation(http_client, gen["id"])

        resp = await http_client.post(
            f"/v1/generations/{gen['id']}/clone", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    # -------------------------------------------------------------------
    # Team-Scoped Generations (GA-11 through GA-15)
    # -------------------------------------------------------------------

    async def test_ga11_team_member_can_see_team_generations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """GA-11: Team member can see team-scoped generations."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team, invite member, accept
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

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

        # Owner creates team-scoped generation
        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), owner=team_urn
        )

        # Invitee (now member) can see team generations
        resp = await http_client.get("/v1/generations", headers=invitee.auth_headers())
        assert resp.status_code == 200
        generation_ids = {g["id"] for g in resp.json()}
        assert gen["id"] in generation_ids

    async def test_ga12_team_member_can_get_team_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """GA-12: Team member can GET a specific team-scoped generation."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team + invite member
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        inv_id = resp.json()["id"]
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), owner=team_urn
        )

        resp = await http_client.get(
            f"/v1/generations/{gen['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["id"] == gen["id"]

    async def test_ga13_non_member_cannot_see_team_generations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """GA-13: Non-member cannot see team-scoped generations."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team (invitee is NOT a member)
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), owner=team_urn
        )

        resp = await http_client.get("/v1/generations", headers=invitee.auth_headers())
        assert resp.status_code == 200
        generation_ids = {g["id"] for g in resp.json()}
        assert gen["id"] not in generation_ids

    async def test_ga14_non_member_cannot_get_team_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """GA-14: Non-member cannot GET a team-scoped generation directly."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), owner=team_urn
        )

        resp = await http_client.get(
            f"/v1/generations/{gen['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ga15_starter_cannot_create_team_scoped_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """GA-15: Starter user cannot create a generation with team URN owner."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create a team (invitee stays starter, not invited)
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

        # Starter tries to create generation with team owner
        resp = await http_client.post(
            "/v1/generations",
            json={"spec": {"prompt": "test"}, "owner": team_urn},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403]

    # -------------------------------------------------------------------
    # Auth Method Variations (GA-16 through GA-20)
    # -------------------------------------------------------------------

    async def test_ga16_no_auth_create_generation_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-16: POST /v1/generations without auth -> 401."""
        resp = await http_client.post(
            "/v1/generations",
            json={"spec": {"prompt": "test"}},
        )
        assert resp.status_code == 401

    async def test_ga17_no_auth_list_generations_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-17: GET /v1/generations without auth -> 401."""
        resp = await http_client.get("/v1/generations")
        assert resp.status_code == 401

    async def test_ga18_no_auth_cancel_generation_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-18: POST /v1/generations/:id/cancel without auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.post(f"/v1/generations/{fake_id}/cancel")
        assert resp.status_code == 401

    async def test_ga19_api_key_can_create_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-19: API key can create ephemeral generation."""
        owner = seed_users.owner

        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        gen = await create_ephemeral_generation(http_client, api_headers)
        assert gen["status"] == "queued"

    async def test_ga20_revoked_api_key_cannot_access_generations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GA-20: Revoked API key cannot access generations."""
        owner = seed_users.owner

        key_id, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # Verify key works
        resp = await http_client.get("/v1/generations", headers=api_headers)
        assert resp.status_code == 200

        # Revoke key
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Key no longer works
        resp = await http_client.get("/v1/generations", headers=api_headers)
        assert resp.status_code == 401
