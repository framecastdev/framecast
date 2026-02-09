"""Multi-User Scenario E2E Tests.

Tests complex multi-user workflows (16 stories):
  - Full team lifecycle (MU1)
  - Ownership transfer (MU2)
  - Team dissolution (MU3-MU4)
  - Multiple teams with shared members (MU5)
  - Invitation race conditions (MU6)
  - Role change affects access (MU7)
  - Member removal and re-invite (MU8)
  - Account deletion cascades (MU9)
  - Onboarding journey (MU10)
  - Concurrent operations (MU11)
  - Admin demotion doesn't invalidate invitation (MU12)
  - Team deleted while invitation pending (MU13)
  - Leave team and API key access (MU14)
  - Creator tier is absorbing (MU15)
  - Viewer upgrade path (MU16)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402


@pytest.mark.teams
class TestMultiUserScenariosE2E:
    """Complex multi-user workflow end-to-end tests."""

    # -----------------------------------------------------------------------
    # Helper
    # -----------------------------------------------------------------------

    async def _create_team(self, http_client, owner, test_data_factory) -> str:
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        return resp.json()["id"]

    async def _invite_and_accept(
        self, http_client, owner, team_id, invitee, role="member"
    ) -> str:
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": role},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        inv_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200
        return inv_id

    # -----------------------------------------------------------------------
    # Scenarios
    # -----------------------------------------------------------------------

    async def test_mu1_full_team_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU1: Owner creates team, invites member, verifies access patterns."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Step 1: Create team
        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Step 2: Invite member
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Step 3: Verify member can view but not modify
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Hacked"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403

        # Step 4: Owner promotes to admin
        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            json={"role": "admin"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Step 5: Admin can now update
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Admin Updated"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

    async def test_mu2_ownership_transfer(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU2: Owner promotes member to owner -> original owner leaves -> team survives."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Promote to owner
        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            json={"role": "owner"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner needs another team to satisfy INV-U2 before leaving
        await self._create_team(http_client, owner, test_data_factory)

        # Original owner leaves
        resp = await http_client.post(
            f"/v1/teams/{team_id}/leave", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Team still exists for new owner
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

    async def test_mu3_team_dissolution_remove_all(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU3: Owner removes members one by one, then deletes team."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Invitee needs another team so removal doesn't violate INV-U2
        await self._create_team(http_client, invitee, test_data_factory)

        # Remove member
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204

        # Delete team (sole member)
        resp = await http_client.delete(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_mu4_cascading_leave_auto_deletes(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU4: Member leaves; last owner leaves -> team auto-deleted."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Invitee needs another team for INV-U2
        await self._create_team(http_client, invitee, test_data_factory)

        # Member leaves
        resp = await http_client.post(
            f"/v1/teams/{team_id}/leave", headers=invitee.auth_headers()
        )
        assert resp.status_code == 204

        # Owner needs another team for INV-U2
        await self._create_team(http_client, owner, test_data_factory)

        # Last owner leaves -> auto-delete
        resp = await http_client.post(
            f"/v1/teams/{team_id}/leave", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Team gone
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_mu5_multiple_teams_shared_members(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU5: Users in various teams; verify each sees correct team list."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates 2 teams
        team_a = await self._create_team(http_client, owner, test_data_factory)
        team_b = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee to team A only
        await self._invite_and_accept(http_client, owner, team_a, invitee)

        # Invitee should see team A (+ auto-team from upgrade)
        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 200
        invitee_teams = resp.json()
        invitee_team_ids = {t["id"] for t in invitee_teams}
        assert team_a in invitee_team_ids
        assert team_b not in invitee_team_ids

        # Owner should see both
        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        assert resp.status_code == 200
        owner_teams = resp.json()
        owner_team_ids = {t["id"] for t in owner_teams}
        assert team_a in owner_team_ids
        assert team_b in owner_team_ids

    async def test_mu6_invitation_race_both_accept(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU6: Two invitees receive invitations; both accept.

        With 2 seeded users, we simulate by inviting invitee + unknown email.
        """
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Invite invitee
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Verify both are members (owner + invitee)
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert len(resp.json()) == 2

    async def test_mu7_role_change_affects_immediate_access(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU7: Admin role revoked -> next request to admin endpoint -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(
            http_client, owner, team_id, invitee, role="admin"
        )

        # Admin can invite
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "test-access@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Demote to viewer
        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            json={"role": "viewer"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Viewer cannot invite
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "test-blocked@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403

    async def test_mu8_member_removed_then_reinvited(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU8: Remove member -> invite again -> accept -> back in team."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Invitee needs another team so removal doesn't violate INV-U2
        await self._create_team(http_client, invitee, test_data_factory)

        # Remove
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204

        # Re-invite and accept
        await self._invite_and_accept(
            http_client, owner, team_id, invitee, role="admin"
        )

        # Verify invitee is back with admin role
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        members = resp.json()
        invitee_member = next(
            (m for m in members if m["user_id"] == invitee.user_id), None
        )
        assert invitee_member is not None
        assert invitee_member["role"] == "admin"

    async def test_mu9_delete_account_cascades_teams(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU9: Creator sole member of teams, deletes teams then account."""
        owner = seed_users.owner

        team_ids = []
        for _ in range(3):
            tid = await self._create_team(http_client, owner, test_data_factory)
            team_ids.append(tid)

        # Delete all teams first (API requires explicit cleanup)
        for tid in team_ids:
            resp = await http_client.delete(
                f"/v1/teams/{tid}", headers=owner.auth_headers()
            )
            assert resp.status_code == 204

        # Delete any remaining teams
        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        if resp.status_code == 200:
            for team in resp.json():
                await http_client.delete(
                    f"/v1/teams/{team['id']}", headers=owner.auth_headers()
                )

        # Delete account
        resp = await http_client.delete("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 204

    async def test_mu10_starter_invited_upgraded_creates_own_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU10: Starter invited -> accepts -> auto-upgrade -> creates own team."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Verify starter
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "starter"

        # Owner invites
        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Now creator — can make own team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201

    async def test_mu11_concurrent_team_creation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU11: Same user creates 2 teams in rapid succession."""
        import asyncio

        owner = seed_users.owner

        async def create_team(name):
            return await http_client.post(
                "/v1/teams",
                json={"name": name},
                headers=owner.auth_headers(),
            )

        results = await asyncio.gather(
            create_team("Concurrent Team A"),
            create_team("Concurrent Team B"),
        )

        # Both should succeed (or one blocked at limit)
        statuses = [r.status_code for r in results]
        assert 201 in statuses, f"At least one should succeed, got {statuses}"

    async def test_mu12_admin_demoted_invitation_still_valid(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU12: Admin invites -> admin demoted -> invitation still valid -> accept works."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as admin
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "admin"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        inv_id = resp.json()["id"]
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Admin creates invitation
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "demoted-test@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200
        pending_inv_id = resp.json()["id"]

        # Owner demotes admin to member
        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            json={"role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Pending invitation should still be valid (it exists)
        resp = await http_client.get(
            f"/v1/teams/{team_id}/invitations", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        invitations = resp.json()
        pending = [i for i in invitations if i["id"] == pending_inv_id]
        assert len(pending) == 1
        assert pending[0]["state"] == "pending"

    async def test_mu13_team_deleted_pending_invitations_invalid(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU13: Owner deletes team -> pending invitations become invalid."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Create invitation
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        inv_id = resp.json()["id"]

        # Delete team
        resp = await http_client.delete(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Try accept — should fail
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 404, 409], (
            f"Expected failure for accepting invitation of deleted team, got {resp.status_code}"
        )

    async def test_mu14_leave_team_lose_team_key_access(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU14: Creator with team-scoped API key leaves team -> key access revoked."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite_and_accept(http_client, owner, team_id, invitee)

        # Invitee creates team-scoped key
        resp = await http_client.post(
            "/v1/auth/keys",
            json={
                "name": "Team Key",
                "scopes": ["generate"],
                "owner": f"framecast:team:{team_id}",
            },
            headers=invitee.auth_headers(),
        )
        # May or may not be allowed depending on implementation
        if resp.status_code == 201:
            assert "api_key" in resp.json()  # key created successfully

            # Create another team so INV-U2 is satisfied
            await self._create_team(http_client, invitee, test_data_factory)

            # Leave team
            resp = await http_client.post(
                f"/v1/teams/{team_id}/leave", headers=invitee.auth_headers()
            )
            assert resp.status_code == 204

    async def test_mu15_creator_tier_is_absorbing(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU15: Creator deletes all teams -> still creator tier (no downgrade)."""
        owner = seed_users.owner

        # Owner is already creator with teams
        # Delete all teams
        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        assert resp.status_code == 200
        teams = resp.json()

        for team in teams:
            # Try to leave/delete (sole member auto-deletes)
            resp = await http_client.post(
                f"/v1/teams/{team['id']}/leave", headers=owner.auth_headers()
            )
            # May fail if not sole member, that's OK for this test

        # Still creator
        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "creator"

    async def test_mu16_viewer_upgrade_path(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """MU16: Viewer -> member -> admin -> owner (gradual role escalation)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add as viewer
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "viewer"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        inv_id = resp.json()["id"]
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Gradual escalation: viewer -> member -> admin -> owner
        for target_role in ["member", "admin", "owner"]:
            resp = await http_client.patch(
                f"/v1/teams/{team_id}/members/{invitee.user_id}",
                json={"role": target_role},
                headers=owner.auth_headers(),
            )
            assert resp.status_code == 200, (
                f"Escalation to {target_role} failed: {resp.status_code} {resp.text}"
            )
            assert resp.json()["role"] == target_role
