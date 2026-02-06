"""Team Management E2E Tests.

Tests the team management user journeys:
  - List teams, list members, leave team
  - Last owner cannot leave (INV-T2)
  - Multi-team visibility
"""

import sys
from pathlib import Path

# Add parent directory to path for utils imports
sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402


@pytest.mark.teams
@pytest.mark.real_services
class TestTeamManagementE2E:
    """Team management end-to-end tests."""

    async def test_team_collaboration_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """
        Full lifecycle: create team -> list teams -> invite member -> accept ->
        list members -> member leaves -> verify cleanup.
        """
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Step 1: Owner creates a team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200, (
            f"Team creation failed: {resp.status_code} {resp.text}"
        )
        team = resp.json()
        team_id = team["id"]

        # Step 2: Owner lists teams — should see 1 team with role "owner"
        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        assert resp.status_code == 200, (
            f"List teams failed: {resp.status_code} {resp.text}"
        )
        teams = resp.json()
        assert len(teams) == 1
        assert teams[0]["id"] == team_id
        assert teams[0]["user_role"] == "owner"

        # Step 3: Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, f"Invite failed: {resp.status_code} {resp.text}"
        invitation_id = resp.json()["id"]

        # Step 4: Invitee accepts invitation
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200, f"Accept failed: {resp.status_code} {resp.text}"

        # Step 5: Invitee lists teams — should see 1 team with role "member"
        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 200, (
            f"Invitee list teams failed: {resp.status_code} {resp.text}"
        )
        invitee_teams = resp.json()
        assert len(invitee_teams) == 1
        assert invitee_teams[0]["id"] == team_id
        assert invitee_teams[0]["user_role"] == "member"

        # Step 6: Owner lists members — should see 2 members with enriched user fields
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200, (
            f"List members failed: {resp.status_code} {resp.text}"
        )
        members = resp.json()
        assert len(members) == 2
        member_roles = {m["role"] for m in members}
        assert "owner" in member_roles
        assert "member" in member_roles
        # Verify enriched user fields are present
        for m in members:
            assert "user_email" in m, f"Missing user_email in member response: {m}"
            assert m["user_email"], "user_email should not be empty"

        # Step 7: Invitee also lists members — same result (any role can view)
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200, (
            f"Invitee list members failed: {resp.status_code} {resp.text}"
        )
        members_from_invitee = resp.json()
        assert len(members_from_invitee) == 2
        for m in members_from_invitee:
            assert "user_email" in m, f"Missing user_email in member response: {m}"

        # Step 8: Invitee leaves team
        resp = await http_client.post(
            f"/v1/teams/{team_id}/leave", headers=invitee.auth_headers()
        )
        assert resp.status_code == 204, (
            f"Leave team failed: {resp.status_code} {resp.text}"
        )

        # Step 9: Owner lists members — should see only 1 (self)
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        members_after_leave = resp.json()
        assert len(members_after_leave) == 1
        assert members_after_leave[0]["role"] == "owner"

        # Step 10: Invitee lists teams — should be empty
        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json() == []

    async def test_last_owner_cannot_leave(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """INV-T2: Last owner cannot leave team — must always have >= 1 owner."""
        owner = seed_users.owner

        # Step 1: Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        team_id = resp.json()["id"]

        # Step 2: Owner tries to leave — should be rejected (409 Conflict)
        resp = await http_client.post(
            f"/v1/teams/{team_id}/leave", headers=owner.auth_headers()
        )
        assert resp.status_code == 409, (
            f"Expected 409 Conflict for last owner leaving, got {resp.status_code} {resp.text}"
        )

        # Step 3: Owner still listed as member
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        members = resp.json()
        assert len(members) == 1
        assert members[0]["role"] == "owner"

    async def test_list_teams_shows_multiple_teams(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """User can see all teams they belong to."""
        owner = seed_users.owner

        # Step 1: Owner creates team A
        team_a_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_a_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        team_a_id = resp.json()["id"]

        # Step 2: Owner creates team B
        team_b_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_b_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        team_b_id = resp.json()["id"]

        # Step 3: Owner lists teams — should see both
        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        assert resp.status_code == 200
        teams = resp.json()
        assert len(teams) == 2

        returned_ids = {t["id"] for t in teams}
        assert team_a_id in returned_ids
        assert team_b_id in returned_ids

        # Both should have role "owner"
        for t in teams:
            assert t["user_role"] == "owner"
