"""Job Access Control E2E Tests.

Tests permissions and RBAC for jobs (20 stories):
  - Owner-only access (JA-01 through JA-05)
  - Cross-user isolation (JA-06 through JA-10)
  - Team-scoped jobs (JA-11 through JA-15)
  - Auth method variations (JA-16 through JA-20)
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
    complete_job,
    create_ephemeral_job,
)


@pytest.mark.job_access
class TestJobAccessControlE2E:
    """Job access control end-to-end tests."""

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
    # Owner-Only Access (JA-01 through JA-05)
    # -------------------------------------------------------------------

    async def test_ja01_owner_can_get_own_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-01: Owner can GET their own job."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        resp = await http_client.get(
            f"/v1/jobs/{job['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["id"] == job["id"]

    async def test_ja02_owner_can_cancel_own_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-02: Owner can cancel their own job."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        resp = await http_client.post(
            f"/v1/jobs/{job['id']}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

    async def test_ja03_owner_can_delete_own_completed_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-03: Owner can delete their own completed job."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        await complete_job(http_client, job["id"])

        resp = await http_client.delete(
            f"/v1/jobs/{job['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_ja04_owner_can_clone_own_completed_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-04: Owner can clone their own completed job."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        await complete_job(http_client, job["id"])

        resp = await http_client.post(
            f"/v1/jobs/{job['id']}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201

    async def test_ja05_owner_can_list_own_jobs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-05: Owner can list their own jobs."""
        owner = seed_users.owner

        await create_ephemeral_job(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/jobs", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert len(resp.json()) >= 1

    # -------------------------------------------------------------------
    # Cross-User Isolation (JA-06 through JA-10)
    # -------------------------------------------------------------------

    async def test_ja06_other_user_cannot_get_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-06: Another user cannot GET someone else's job."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        resp = await http_client.get(
            f"/v1/jobs/{job['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ja07_other_user_cannot_cancel_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-07: Another user cannot cancel someone else's job."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        resp = await http_client.post(
            f"/v1/jobs/{job['id']}/cancel", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ja08_other_user_cannot_delete_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-08: Another user cannot delete someone else's job."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        await complete_job(http_client, job["id"])

        resp = await http_client.delete(
            f"/v1/jobs/{job['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ja09_other_user_jobs_not_in_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-09: Another user's jobs don't appear in your list."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        job = await create_ephemeral_job(http_client, owner.auth_headers())

        resp = await http_client.get("/v1/jobs", headers=invitee.auth_headers())
        assert resp.status_code == 200
        job_ids = {j["id"] for j in resp.json()}
        assert job["id"] not in job_ids

    async def test_ja10_other_user_cannot_clone_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-10: Another user cannot clone someone else's job."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        await complete_job(http_client, job["id"])

        resp = await http_client.post(
            f"/v1/jobs/{job['id']}/clone", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    # -------------------------------------------------------------------
    # Team-Scoped Jobs (JA-11 through JA-15)
    # -------------------------------------------------------------------

    async def test_ja11_team_member_can_see_team_jobs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """JA-11: Team member can see team-scoped jobs."""
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

        # Owner creates team-scoped job
        job = await create_ephemeral_job(
            http_client, owner.auth_headers(), owner=team_urn
        )

        # Invitee (now member) can see team jobs
        resp = await http_client.get("/v1/jobs", headers=invitee.auth_headers())
        assert resp.status_code == 200
        job_ids = {j["id"] for j in resp.json()}
        assert job["id"] in job_ids

    async def test_ja12_team_member_can_get_team_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """JA-12: Team member can GET a specific team-scoped job."""
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

        job = await create_ephemeral_job(
            http_client, owner.auth_headers(), owner=team_urn
        )

        resp = await http_client.get(
            f"/v1/jobs/{job['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["id"] == job["id"]

    async def test_ja13_non_member_cannot_see_team_jobs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """JA-13: Non-member cannot see team-scoped jobs."""
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

        job = await create_ephemeral_job(
            http_client, owner.auth_headers(), owner=team_urn
        )

        resp = await http_client.get("/v1/jobs", headers=invitee.auth_headers())
        assert resp.status_code == 200
        job_ids = {j["id"] for j in resp.json()}
        assert job["id"] not in job_ids

    async def test_ja14_non_member_cannot_get_team_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """JA-14: Non-member cannot GET a team-scoped job directly."""
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

        job = await create_ephemeral_job(
            http_client, owner.auth_headers(), owner=team_urn
        )

        resp = await http_client.get(
            f"/v1/jobs/{job['id']}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [403, 404]

    async def test_ja15_starter_cannot_create_team_scoped_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """JA-15: Starter user cannot create a job with team URN owner."""
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

        # Starter tries to create job with team owner
        resp = await http_client.post(
            "/v1/generate",
            json={"spec": {"prompt": "test"}, "owner": team_urn},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403]

    # -------------------------------------------------------------------
    # Auth Method Variations (JA-16 through JA-20)
    # -------------------------------------------------------------------

    async def test_ja16_no_auth_create_job_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-16: POST /v1/generate without auth -> 401."""
        resp = await http_client.post(
            "/v1/generate",
            json={"spec": {"prompt": "test"}},
        )
        assert resp.status_code == 401

    async def test_ja17_no_auth_list_jobs_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-17: GET /v1/jobs without auth -> 401."""
        resp = await http_client.get("/v1/jobs")
        assert resp.status_code == 401

    async def test_ja18_no_auth_cancel_job_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-18: POST /v1/jobs/:id/cancel without auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.post(f"/v1/jobs/{fake_id}/cancel")
        assert resp.status_code == 401

    async def test_ja19_api_key_can_create_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-19: API key can create ephemeral job."""
        owner = seed_users.owner

        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        job = await create_ephemeral_job(http_client, api_headers)
        assert job["status"] == "queued"

    async def test_ja20_revoked_api_key_cannot_access_jobs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JA-20: Revoked API key cannot access jobs."""
        owner = seed_users.owner

        key_id, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # Verify key works
        resp = await http_client.get("/v1/jobs", headers=api_headers)
        assert resp.status_code == 200

        # Revoke key
        resp = await http_client.delete(
            f"/v1/auth/keys/{key_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Key no longer works
        resp = await http_client.get("/v1/jobs", headers=api_headers)
        assert resp.status_code == 401
