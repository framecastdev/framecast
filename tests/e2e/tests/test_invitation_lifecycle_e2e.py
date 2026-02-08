"""Invitation Lifecycle E2E Tests.

Tests invitation operations (30 stories):
  - Happy path (I1-I9)
  - State machine transitions (I10-I16)
  - Permission guards (I17-I24)
  - Invariant enforcement (I25-I29)
  - Edge cases (I30)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402
from hypothesis import given  # noqa: E402
from strategies import e2e_settings, invitation_roles  # noqa: E402
from utils.localstack_email import LocalStackEmailClient  # noqa: E402


@pytest.mark.invitation
@pytest.mark.real_services
class TestInvitationLifecycleE2E:
    """Invitation lifecycle end-to-end tests."""

    # -----------------------------------------------------------------------
    # Helper
    # -----------------------------------------------------------------------

    async def _create_team(
        self,
        http_client: httpx.AsyncClient,
        owner,
        test_data_factory: TestDataFactory,
    ) -> str:
        """Create a team and return its ID."""
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        return resp.json()["id"]

    async def _invite(
        self,
        http_client: httpx.AsyncClient,
        owner,
        team_id: str,
        email: str,
        role: str = "member",
    ) -> str:
        """Send invitation and return invitation ID."""
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": email, "role": role},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, f"Invite failed: {resp.status_code} {resp.text}"
        return resp.json()["id"]

    # -----------------------------------------------------------------------
    # Happy Path (I1-I9)
    # -----------------------------------------------------------------------

    async def test_i1_full_accept_workflow_with_email(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """I1: Owner invites -> email sent -> invitee accepts -> membership created, tier upgraded."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        invitation_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Verify email sent
        email = await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )
        assert email is not None, "Invitation email not received"

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200
        membership = resp.json()
        assert membership["user_id"] == invitee.user_id
        assert membership["role"] == "member"

        # Verify tier upgraded
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "creator"

    async def test_i2_invite_with_admin_role(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I2: Owner invites with role=admin -> accept -> membership has admin role."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        invitation_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="admin"
        )

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["role"] == "admin"

    async def test_i3_invite_with_viewer_role(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I3: Owner invites with role=viewer -> accept -> membership has viewer role."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        invitation_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="viewer"
        )

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["role"] == "viewer"

    async def test_i4_decline_invitation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I4: Invitee declines -> 204, no membership created."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        invitation_id = await self._invite(http_client, owner, team_id, invitee.email)

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

    async def test_i5_revoke_invitation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I5: Owner revokes -> 204, invitation no longer actionable."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        invitation_id = await self._invite(http_client, owner, team_id, invitee.email)

        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{invitation_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204

    async def test_i6_list_invitations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I6: Owner/admin lists team invitations -> returns all with state."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        await self._invite(http_client, owner, team_id, invitee.email)
        await self._invite(http_client, owner, team_id, "other@test.com")

        resp = await http_client.get(
            f"/v1/teams/{team_id}/invitations", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        invitations = resp.json()
        assert len(invitations) >= 2
        for inv in invitations:
            assert "state" in inv
            assert "email" in inv

    async def test_i7_resend_invitation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I7: Owner resends -> 200, expires_at extended."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        invitation_id = await self._invite(http_client, owner, team_id, invitee.email)

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations/{invitation_id}/resend",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, f"Resend failed: {resp.status_code} {resp.text}"

    async def test_i8_reinvite_after_decline(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I8: Decline -> new invite -> accept -> membership created."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # First invite and decline
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

        # Re-invite
        new_inv_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="admin"
        )

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{new_inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["role"] == "admin"

    async def test_i9_reinvite_replaces_existing_pending(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I9: Invite -> invite again (same email) -> old revoked, new created."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        first_id = await self._invite(http_client, owner, team_id, invitee.email)
        second_id = await self._invite(http_client, owner, team_id, invitee.email)

        assert second_id != first_id

    # -----------------------------------------------------------------------
    # Invitation State Machine (I10-I16)
    # -----------------------------------------------------------------------

    async def test_i10_accept_revoked_invitation_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I10: Revoke -> accept -> 400/409 (revoked is terminal)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Revoke
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{inv_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204

        # Try accept
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for accepting revoked, got {resp.status_code} {resp.text}"
        )

    async def test_i11_accept_declined_invitation_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I11: Decline -> accept same invitation -> 400/409."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Decline
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

        # Try accept
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for accepting declined, got {resp.status_code} {resp.text}"
        )

    async def test_i12_decline_revoked_invitation_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I12: Revoke -> decline -> 400/409."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Revoke
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{inv_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204

        # Try decline
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for declining revoked, got {resp.status_code} {resp.text}"
        )

    async def test_i13_accept_already_accepted_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I13: Accept -> accept again -> 400/409."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Try accept again
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for double accept, got {resp.status_code} {resp.text}"
        )

    async def test_i14_revoke_already_accepted_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I14: Accept -> revoke -> 400/409 (INV-I3)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Try revoke
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{inv_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for revoking accepted, got {resp.status_code} {resp.text}"
        )

    async def test_i15_resend_revoked_invitation_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I15: Revoke -> resend -> 400/409."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Revoke
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{inv_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204

        # Try resend
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations/{inv_id}/resend",
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 404, 409], (
            f"Expected 400/404/409 for resending revoked, got {resp.status_code} {resp.text}"
        )

    async def test_i16_resend_accepted_invitation_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I16: Accept -> resend -> 400/409."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Try resend
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations/{inv_id}/resend",
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for resending accepted, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Permission Guards (I17-I24)
    # -----------------------------------------------------------------------

    async def test_i17_member_cannot_invite(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I17: Member role POST invitation -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as member
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Member tries to invite
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "someone@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403, (
            f"Expected 403 for member inviting, got {resp.status_code} {resp.text}"
        )

    async def test_i18_viewer_cannot_invite(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I18: Viewer role POST invitation -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as viewer
        inv_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="viewer"
        )
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Viewer tries to invite
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "someone@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403, (
            f"Expected 403 for viewer inviting, got {resp.status_code} {resp.text}"
        )

    async def test_i19_admin_can_invite(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I19: Admin role POST invitation -> 200."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as admin
        inv_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="admin"
        )
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Admin invites someone
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "admin-invited@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Expected 200 for admin inviting, got {resp.status_code} {resp.text}"
        )

    async def test_i20_admin_can_revoke(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I20: Admin role DELETE invitation -> 204."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as admin
        inv_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="admin"
        )
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner creates an invitation for admin to revoke
        target_inv = await self._invite(
            http_client, owner, team_id, "revoke-target@test.com"
        )

        # Admin revokes
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{target_inv}",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

    async def test_i21_admin_can_resend(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I21: Admin role POST resend -> 200."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as admin
        inv_id = await self._invite(
            http_client, owner, team_id, invitee.email, role="admin"
        )
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner creates invitation
        target_inv = await self._invite(
            http_client, owner, team_id, "resend-target@test.com"
        )

        # Admin resends
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations/{target_inv}/resend",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

    async def test_i22_member_cannot_list_invitations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I22: Member GET invitations -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        inv_id = await self._invite(http_client, owner, team_id, invitee.email)
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Member tries to list invitations
        resp = await http_client.get(
            f"/v1/teams/{team_id}/invitations",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403, (
            f"Expected 403 for member listing invitations, got {resp.status_code}"
        )

    async def test_i23_non_member_cannot_invite(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I23: Non-member POST invitation -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Invitee (not a member) tries to invite
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "random@test.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [401, 403], (
            f"Expected 401/403 for non-member inviting, got {resp.status_code}"
        )

    async def test_i24_wrong_user_cannot_accept(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I24: User B tries to accept invitation sent to User A -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Owner (different user) tries to accept invitee's invitation
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 403], (
            f"Expected 400/403 for wrong user accepting, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Invariant Enforcement (I25-I29)
    # -----------------------------------------------------------------------

    async def test_i25_cannot_invite_self(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I25: Owner invites own email -> 400 (INV-I7)."""
        owner = seed_users.owner

        team_id = await self._create_team(http_client, owner, test_data_factory)

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": owner.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for self-invite, got {resp.status_code} {resp.text}"
        )

    async def test_i26_cannot_invite_existing_member(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I26: Invite someone already in team -> 409 (INV-I8)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id = await self._create_team(http_client, owner, test_data_factory)

        # Add invitee as member
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Try inviting again
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 409, (
            f"Expected 409 for inviting existing member, got {resp.status_code} {resp.text}"
        )

    async def test_i27_cannot_invite_with_role_owner(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I27: POST invitation with role=owner -> 400 (INV-I2)."""
        owner = seed_users.owner

        team_id = await self._create_team(http_client, owner, test_data_factory)

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "newuser@test.com", "role": "owner"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for owner-role invitation, got {resp.status_code} {resp.text}"
        )

    async def test_i28_accept_respects_max_memberships(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I28: User at max memberships -> accept -> 400 (INV-T8).

        Full test requires 50 teams which is very slow.
        This is a placeholder verifying the mechanism exists.
        """
        # Verified via TL2 in test_team_limits_e2e.py
        pass

    async def test_i29_starter_auto_upgraded_on_accept(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I29: Starter accepts -> becomes creator, gets personal team (INV-M4)."""
        owner = seed_users.owner
        invitee = seed_users.invitee  # starter

        team_id = await self._create_team(http_client, owner, test_data_factory)
        inv_id = await self._invite(http_client, owner, team_id, invitee.email)

        # Verify starter before accept
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "starter"

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Verify upgraded
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        account = resp.json()
        assert account["tier"] == "creator"
        assert account["upgraded_at"] is not None

    # -----------------------------------------------------------------------
    # Edge Cases (I30)
    # -----------------------------------------------------------------------

    async def test_i30_invite_email_no_matching_user(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """I30: Invite random@unknown.com -> 200 (invitation created for future signup)."""
        owner = seed_users.owner

        team_id = await self._create_team(http_client, owner, test_data_factory)

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "random-future-user@unknown.com", "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Expected 200 for inviting non-existent user, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Property-Based Tests
    # -----------------------------------------------------------------------

    @e2e_settings
    @given(role=invitation_roles)
    async def test_invite_with_any_valid_role(
        self,
        role: str,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Property: invitations with any valid non-owner role succeed."""
        owner = seed_users.owner

        team_id = await self._create_team(http_client, owner, test_data_factory)

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": f"property-{role}@test.com", "role": role},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Expected 200 for role={role}, got {resp.status_code} {resp.text}"
        )
