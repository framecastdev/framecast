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
        assert resp.status_code == 200, (
            f"Team creation failed: {resp.status_code} {resp.text}"
        )
        team = resp.json()
        team_id = team["id"]

        # Step 2: Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invite",
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
        resp = await http_client.put(
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
        assert resp.status_code == 200
        team_id = resp.json()["id"]

        # Owner invites invitee
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invite",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        # Invitee declines
        resp = await http_client.put(
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
        assert resp.status_code == 200
        team_id = resp.json()["id"]

        # First invitation succeeds
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invite",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Second invitation to same email should be rejected
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invite",
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
        assert resp.status_code == 200
        team_id = resp.json()["id"]

        # Invitee (not a member) tries to invite someone — should fail
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invite",
            json={"email": "random@example.com", "role": "member"},
            headers=invitee.auth_headers(),
        )
        # Should get authorization error (not a member of the team)
        assert resp.status_code in [401, 403]
