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
from conftest import SeededUsers, TestDataFactory, UserPersona  # noqa: E402
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
        assert resp.status_code == 200, f"Team creation failed: {resp.status_code} {resp.text}"
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
        # LocalStack filters by sender, so also try fetching all emails
        if email is None:
            all_emails = await localstack_email_client.get_emails(
                "invitations@framecast.app"
            )
            for e in all_emails:
                to_list = e.to if isinstance(e.to, list) else [e.to]
                if invitee.email in to_list:
                    email = e
                    break

        assert email is not None, (
            f"Invitation email not found for {invitee.email}"
        )

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
        resp = await http_client.get(
            "/v1/account", headers=invitee.auth_headers()
        )
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


@pytest.mark.invitation
@pytest.mark.real_services
class TestInvitationEmailContent:
    """Test email content and extraction from LocalStack."""

    async def test_invitation_url_extraction_methods(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test that extraction methods work on sample email content."""
        test_uuid = "550e8400-e29b-41d4-a716-446655440000"  # pragma: allowlist secret
        base_url = "https://framecast.app/teams/tm_123/invitations"
        sample_body = f"""
        You have been invited to join Test Team as an admin.
        Click here to accept: {base_url}/{test_uuid}/accept
        """

        url = localstack_email_client.extract_invitation_url(sample_body)
        inv_id = localstack_email_client.extract_invitation_id(sample_body)

        assert url is not None
        assert "accept" in url
        assert inv_id == test_uuid

    async def test_invitation_url_extraction_edge_cases(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test invitation URL extraction with various email formats."""
        base = "https://framecast.app/teams/tm_123/invitations/inv_456/accept"
        test_cases = [
            {
                "name": "HTML email with href",
                "body": f'<a href="{base}">Accept</a>',
                "expect_url": True,
            },
            {
                "name": "Plain text with full URL",
                "body": f"Click here: {base}",
                "expect_url": True,
            },
            {
                "name": "Relative URL",
                "body": "Accept: /teams/tm_123/invitations/inv_456/accept",
                "expect_url": True,
            },
            {
                "name": "No URL",
                "body": "This is an invitation email with no links.",
                "expect_url": False,
            },
        ]

        for test_case in test_cases:
            url = localstack_email_client.extract_invitation_url(test_case["body"])

            if test_case["expect_url"]:
                assert url is not None, f"Expected URL in: {test_case['name']}"
                assert "accept" in url, f"URL should contain 'accept': {url}"
            else:
                assert url is None, f"Should not extract URL from: {test_case['name']}"


@pytest.mark.invitation
@pytest.mark.real_services
class TestInvitationErrorHandling:
    """Test error handling in invitation workflow."""

    async def test_email_not_found_returns_none(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test that missing emails return None gracefully."""
        nonexistent = "nonexistent@test.local"
        email = await localstack_email_client.get_latest_email(nonexistent)
        assert email is None

        email = await localstack_email_client.wait_for_email(
            "nonexistent@test.local", timeout=1.0
        )
        assert email is None

    async def test_malformed_email_content_returns_none(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test that malformed content returns None gracefully."""
        test_uuid = "12345678-1234-1234-1234-123456789012"  # pragma: allowlist secret
        malformed_bodies = [
            "",
            "No URLs here",
            f"random text with uuid {test_uuid} but no invitation",
            "<html><body>Broken HTML",
        ]

        for body in malformed_bodies:
            url = localstack_email_client.extract_invitation_url(body)
            inv_id = localstack_email_client.extract_invitation_id(body)

            assert url is None
            assert inv_id is None
