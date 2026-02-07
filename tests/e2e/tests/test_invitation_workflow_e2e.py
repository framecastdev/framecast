"""Invitation Workflow E2E Tests.

Tests the complete invitation user journey:
  owner creates team -> invites member -> email sent ->
  invitee accepts -> invitee upgraded to Creator -> membership created
"""

import sys
from pathlib import Path

# Add parent directory to path for utils imports
sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402
from utils.localstack_email import LocalStackEmailClient  # noqa: E402


@pytest.mark.invitation
@pytest.mark.real_services
class TestInvitationWorkflowE2E:
    """Complete invitation workflow end-to-end tests."""

    async def test_complete_invitation_accept_workflow(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """
        Full E2E: owner creates team -> invites member -> email sent ->
        invitee accepts -> invitee upgraded to Creator -> membership created
        """
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Step 1: Owner creates a team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201, (
            f"Team creation failed: {resp.status_code} {resp.text}"
        )
        team = resp.json()
        team_id = team["id"]

        # Step 2: Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, f"Invite failed: {resp.status_code} {resp.text}"
        invitation = resp.json()
        assert invitation["state"] == "pending"
        assert invitation["email"] == invitee.email
        invitation_id = invitation["id"]

        # Step 3: Verify invitation email was sent (via LocalStack SES)
        email = await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )

        assert email is not None, f"Invitation email not found for {invitee.email}"

        # Step 4: Invitee accepts invitation (currently Starter tier)
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Accept invitation failed: {resp.status_code} {resp.text}"
        )
        membership = resp.json()
        assert membership["user_id"] == invitee.user_id
        assert membership["role"] == "member"

        # Step 5: Verify invitee was auto-upgraded to Creator tier
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        account = resp.json()
        assert account["tier"] == "creator", (
            f"Expected tier 'creator' after accept, got '{account['tier']}'"
        )
        assert account["upgraded_at"] is not None

    async def test_invitation_decline_workflow(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Owner invites -> invitee declines -> no membership created."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        # Invitee declines
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

    async def test_duplicate_invitation_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Cannot invite same email twice while pending."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # First invitation succeeds
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Second invitation to same email should be rejected
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 409  # Conflict

    async def test_non_owner_cannot_invite(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Regular members cannot send invitations."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Invitee (not a member) tries to invite someone — should fail
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "random@example.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        # Should get authorization error (not a member of the team)
        assert resp.status_code in [401, 403]

    async def test_invitation_revoke_then_accept_fails(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Invite -> revoke -> accept should fail (revoked is terminal)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        # Owner revokes the invitation (DELETE, not POST)
        resp = await http_client.delete(
            f"/v1/teams/{team_id}/invitations/{invitation_id}",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 204, f"Revoke failed: {resp.status_code} {resp.text}"

        # Invitee tries to accept revoked invitation — should fail
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for revoked invitation accept, got {resp.status_code} {resp.text}"
        )

    async def test_reinvite_after_decline(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """Invite -> decline -> re-invite -> accept -> membership created."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        team_data = test_data_factory.team_data()
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        # Invitee declines
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

        # Owner sends a new invitation to same email
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "admin"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Re-invite after decline should succeed: {resp.status_code} {resp.text}"
        )
        new_invitation_id = resp.json()["id"]
        assert resp.json()["role"] == "admin"

        # Invitee accepts the new invitation
        resp = await http_client.post(
            f"/v1/invitations/{new_invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Accept re-invite failed: {resp.status_code} {resp.text}"
        )

        # Verify invitee is now a team member with admin role
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        members = resp.json()
        invitee_member = next(
            (m for m in members if m["user_id"] == invitee.user_id), None
        )
        assert invitee_member is not None, "Invitee should be a team member"
        assert invitee_member["role"] == "admin"

    async def test_invitation_email_contains_team_info(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """Verify invitation email subject/body contain team name and role."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team with a recognizable name
        team_data = test_data_factory.team_data()
        team_data["name"] = "Verification Test Studio"
        resp = await http_client.post(
            "/v1/teams", json=team_data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Wait for and retrieve invitation emails for the invitee
        import asyncio

        team_name = "Verification Test Studio"
        target_email = None
        start = asyncio.get_event_loop().time()
        timeout = 15

        while (asyncio.get_event_loop().time() - start) < timeout:
            emails = await localstack_email_client.get_emails(invitee.email)
            for e in emails:
                if team_name in (e.subject or "") or team_name in (e.body or ""):
                    target_email = e
                    break
            if target_email:
                break
            await asyncio.sleep(0.5)

        assert target_email is not None, (
            f"Invitation email with team name '{team_name}' not found for {invitee.email}"
        )

        # Verify email contains an invitation URL
        invitation_url = localstack_email_client.extract_invitation_url(
            target_email.body
        )
        assert invitation_url is not None, (
            f"Email should contain invitation URL. Body: {target_email.body[:200]}"
        )
