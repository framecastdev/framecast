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

    async def test_last_owner_leaving_auto_deletes_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Last owner (sole member) leaving auto-deletes the team."""
        owner = seed_users.owner

        # Step 1: Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        team_id = resp.json()["id"]

        # Step 2: Owner leaves — team should be auto-deleted (204)
        resp = await http_client.post(
            f"/v1/teams/{team_id}/leave", headers=owner.auth_headers()
        )
        assert resp.status_code == 204, (
            f"Expected 204 for last owner leaving (auto-delete), got {resp.status_code} {resp.text}"
        )

        # Step 3: Team should no longer exist
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_team_create_update_get_delete_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Full CRUD lifecycle: create -> get -> update name -> get -> delete -> get (404)."""
        owner = seed_users.owner

        # Step 1: Create team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200, f"Create failed: {resp.status_code} {resp.text}"
        team = resp.json()
        team_id = team["id"]
        assert team["name"] == team_data["name"]
        assert "slug" in team
        assert team["user_role"] == "owner"

        # Step 2: Get team
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        fetched = resp.json()
        assert fetched["id"] == team_id
        assert fetched["name"] == team_data["name"]

        # Step 3: Update team name
        new_name = "Updated Team Name"
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": new_name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, f"Update failed: {resp.status_code} {resp.text}"
        updated = resp.json()
        assert updated["name"] == new_name

        # Step 4: Get again to verify update persisted
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["name"] == new_name

        # Step 5: Delete team
        resp = await http_client.delete(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204, f"Delete failed: {resp.status_code} {resp.text}"

        # Step 6: Verify team is gone
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_cross_team_isolation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """User A owns team X, user B owns team Y — A cannot access team Y members."""
        owner = seed_users.owner
        other_user = seed_users.invitee

        # Owner creates team X
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        team_x_id = resp.json()["id"]

        # Other user needs to be creator tier to create a team
        # Upgrade invitee to creator first
        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=other_user.auth_headers(),
        )
        # May already be creator from previous test, so 200 or 409 are both ok
        assert resp.status_code in [200, 409], (
            f"Upgrade failed: {resp.status_code} {resp.text}"
        )

        # Other user creates team Y
        team_data_y = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data_y, headers=other_user.auth_headers()
        )
        assert resp.status_code == 200
        team_y_id = resp.json()["id"]

        # Owner tries to list team Y members — should be forbidden
        resp = await http_client.get(
            f"/v1/teams/{team_y_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 403, (
            f"Expected 403 for cross-team access, got {resp.status_code}"
        )

        # Other user tries to list team X members — should be forbidden
        resp = await http_client.get(
            f"/v1/teams/{team_x_id}/members", headers=other_user.auth_headers()
        )
        assert resp.status_code == 403, (
            f"Expected 403 for cross-team access, got {resp.status_code}"
        )

    async def test_starter_cannot_create_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Starter tier user cannot create teams (requires creator tier)."""
        invitee = seed_users.invitee

        # The invitee starts as starter tier (reset by seed_users fixture)
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=invitee.auth_headers()
        )
        assert resp.status_code == 403, (
            f"Expected 403 for starter creating team, got {resp.status_code} {resp.text}"
        )

        # Verify error structure
        error = resp.json()
        assert "error" in error
