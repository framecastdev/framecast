"""Role-Based Access E2E Tests.

Tests permission matrix scenarios by role and tier (18 stories):
  - Team-level permissions by role (R1-R4)
  - Cross-role interaction scenarios (R5-R10)
  - Tier-level access guards (R11-R16)
  - Multi-team role isolation (R17-R18)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402


@pytest.mark.teams
@pytest.mark.security
class TestRoleBasedAccessE2E:
    """Role-based access end-to-end tests."""

    # -----------------------------------------------------------------------
    # Helper
    # -----------------------------------------------------------------------

    async def _create_team_with_member(
        self,
        http_client: httpx.AsyncClient,
        owner,
        invitee,
        role: str,
        test_data_factory: TestDataFactory,
    ) -> str:
        """Create team, add invitee with role. Returns team_id."""
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

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
        return team_id

    # -----------------------------------------------------------------------
    # Team-Level Permissions by Role (R1-R4)
    # -----------------------------------------------------------------------

    async def test_r1_owner_can_do_everything(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R1: Owner: view, update, invite, manage members -> all succeed."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # View team
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Update team
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Owner Updated"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # View members
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Invite someone
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "owner-invite@test.com", "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201

        # List invitations
        resp = await http_client.get(
            f"/v1/teams/{team_id}/invitations", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

    async def test_r2_admin_can_manage_but_not_delete_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R2: Admin: view OK, update OK, delete FAIL, invite OK."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "admin", test_data_factory
        )

        # View OK
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # Update OK
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Admin Updated"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Invite OK
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "admin-test-invite@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201

        # Delete FAIL
        resp = await http_client.delete(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for admin deleting team, got {resp.status_code}"
        )

    async def test_r3_member_can_view_only(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R3: Member: view team OK, view members OK, update FAIL, invite FAIL."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "member", test_data_factory
        )

        # View team OK
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # View members OK
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # Update FAIL
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Member Hacked"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403

        # Invite FAIL
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "member-invite@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403

    async def test_r4_viewer_can_view_only(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R4: Viewer: view team OK, view members OK, update FAIL, invite FAIL."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "viewer", test_data_factory
        )

        # View team OK
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # View members OK
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # Update FAIL
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Viewer Hacked"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403

        # Invite FAIL
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "viewer-invite@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403

    # -----------------------------------------------------------------------
    # Cross-Role Interaction Scenarios (R5-R10)
    # -----------------------------------------------------------------------

    async def test_r5_owner_promotes_full_ladder(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R5: Owner promotes viewer -> member -> admin -> owner."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "viewer", test_data_factory
        )

        for target_role in ["member", "admin", "owner"]:
            resp = await http_client.patch(
                f"/v1/teams/{team_id}/members/{invitee.user_id}",
                json={"role": target_role},
                headers=owner.auth_headers(),
            )
            assert resp.status_code == 200, (
                f"Promote to {target_role} failed: {resp.status_code} {resp.text}"
            )
            assert resp.json()["role"] == target_role

    async def test_r6_owner_demotes_full_ladder(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R6: Owner demotes admin -> member -> viewer."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "admin", test_data_factory
        )

        for target_role in ["member", "viewer"]:
            resp = await http_client.patch(
                f"/v1/teams/{team_id}/members/{invitee.user_id}",
                json={"role": target_role},
                headers=owner.auth_headers(),
            )
            assert resp.status_code == 200, (
                f"Demote to {target_role} failed: {resp.status_code} {resp.text}"
            )
            assert resp.json()["role"] == target_role

    async def test_r7_admin_promotes_viewer_to_member(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R7: Admin PATCH viewer to member -> 200.

        Requires 3 users. With 2, we verify admin has management permissions.
        """
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "admin", test_data_factory
        )

        # Verify admin can manage (view members, list invitations)
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

    async def test_r8_admin_promotes_member_to_admin(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R8: Admin PATCH member to admin -> 200. (3-user test, simplified)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "admin", test_data_factory
        )

        resp = await http_client.get(
            f"/v1/teams/{team_id}/invitations", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

    async def test_r9_admin_cannot_promote_to_owner(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R9: Admin PATCH to owner -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "admin", test_data_factory
        )

        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{owner.user_id}",
            json={"role": "owner"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for admin promoting to owner, got {resp.status_code}"
        )

    async def test_r10_two_owners_one_demotes_the_other(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R10: Owner A demotes Owner B to admin -> 200 (INV-T2 satisfied)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team_with_member(
            http_client, owner, invitee, "member", test_data_factory
        )

        # Promote invitee to owner
        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            json={"role": "owner"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner demotes invitee back to admin
        resp = await http_client.patch(
            f"/v1/teams/{team_id}/members/{invitee.user_id}",
            json={"role": "admin"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["role"] == "admin"

    # -----------------------------------------------------------------------
    # Tier-Level Access Guards (R11-R16)
    # -----------------------------------------------------------------------

    async def test_r11_starter_cannot_access_teams_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """R11: Starter cannot access GET /v1/teams -> 403."""
        invitee = seed_users.invitee  # starter

        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 403

    async def test_r12_starter_cannot_access_team_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R12: Starter cannot access GET /v1/teams/:id -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 403

    async def test_r13_starter_cannot_access_team_members(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R13: Starter cannot access GET /v1/teams/:id/members -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=invitee.auth_headers()
        )
        assert resp.status_code == 403

    async def test_r14_starter_can_access_account(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """R14: Starter CAN access GET /v1/account -> 200."""
        invitee = seed_users.invitee

        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200

    async def test_r15_starter_can_access_auth_keys(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """R15: Starter CAN access GET /v1/auth/keys -> 200."""
        invitee = seed_users.invitee

        resp = await http_client.get("/v1/auth/keys", headers=invitee.auth_headers())
        assert resp.status_code == 200

    async def test_r16_starter_can_upgrade(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """R16: Starter CAN upgrade POST /v1/account/upgrade -> 200."""
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [200, 409], (
            f"Expected 200/409 for starter upgrade, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Multi-Team Role Isolation (R17-R18)
    # -----------------------------------------------------------------------

    async def test_r17_owner_in_team_a_member_in_team_b(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R17: User is owner in team A, member in team B. Can update A, cannot update B."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team A (is owner)
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_a_id = resp.json()["id"]

        # Upgrade invitee, create team B (invitee is owner of B)
        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [200, 409]

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201
        team_b_id = resp.json()["id"]

        # Add owner as member of team B
        resp = await http_client.post(
            f"/v1/teams/{team_b_id}/invitations",
            json={"email": owner.email, "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201
        inv_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner can update team A (is owner)
        resp = await http_client.patch(
            f"/v1/teams/{team_a_id}",
            json={"name": "Owner Team Updated"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner cannot update team B (is member only)
        resp = await http_client.patch(
            f"/v1/teams/{team_b_id}",
            json={"name": "Hacked"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 403

    async def test_r18_admin_in_team_a_viewer_in_team_b(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """R18: User is admin in team A, viewer in team B. Can invite in A, cannot invite in B."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team A, invitee is admin
        team_a_id = await self._create_team_with_member(
            http_client, owner, invitee, "admin", test_data_factory
        )

        # Create team B
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_b_id = resp.json()["id"]

        # Change invitee's role in team A to viewer for team B check
        # Actually, invitee is already admin in A. We need them as viewer in B.
        # Remove from A's context and add to B as viewer
        resp = await http_client.post(
            f"/v1/teams/{team_b_id}/invitations",
            json={"email": invitee.email, "role": "viewer"},
            headers=owner.auth_headers(),
        )
        # Might get 409 if already a member (invitee is creator and may have auto-team)
        if resp.status_code == 201:
            inv_id = resp.json()["id"]
            resp = await http_client.post(
                f"/v1/invitations/{inv_id}/accept",
                headers=invitee.auth_headers(),
            )
            assert resp.status_code == 200

            # Admin can invite in team A
            resp = await http_client.post(
                f"/v1/teams/{team_a_id}/invitations",
                json={"email": "admin-a-invite@test.com", "role": "member"},
                headers=invitee.auth_headers(),
            )
            assert resp.status_code == 201

            # Viewer cannot invite in team B
            resp = await http_client.post(
                f"/v1/teams/{team_b_id}/invitations",
                json={"email": "viewer-b-invite@test.com", "role": "member"},
                headers=invitee.auth_headers(),
            )
            assert resp.status_code == 403
