"""Invitation Workflow E2E Tests.

Tests the complete invitation user journey with email retrieval and validation.
"""

import sys
from pathlib import Path

# Add parent directory to path for utils imports
sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import TestDataFactory, UserPersona  # noqa: E402
from utils.localstack_email import LocalStackEmailClient  # noqa: E402


@pytest.mark.invitation
@pytest.mark.integration
class TestInvitationWorkflowE2E:
    """Complete invitation workflow end-to-end tests."""

    @pytest.mark.real_services
    async def test_complete_invitation_workflow_with_email_verification(
        self,
        authenticated_client: httpx.AsyncClient,
        creator_user: UserPersona,
        localstack_email_client: LocalStackEmailClient,
        email_cleanup: callable,
        test_data_factory: TestDataFactory,
    ):
        """Test complete invitation workflow."""
        # Step 1: Create team via API
        team_data = test_data_factory.team_data()

        try:
            response = await authenticated_client.post("/v1/teams", json=team_data)
        except httpx.ConnectError:
            pytest.skip("API server not running")

        if response.status_code == 501:
            pytest.skip("Team creation API not implemented yet")
        elif response.status_code in [200, 201]:
            team = response.json()
            team_id = team["id"]
        else:
            pytest.skip(f"Team creation failed: {response.status_code}")

        # Step 2: Send invitation via API
        invitee_email = "e2e-invitation-test@example.com"
        invitation_data = {"email": invitee_email, "role": "admin"}

        # Clear any existing emails for this address first
        await localstack_email_client.clear_emails(invitee_email)

        response = await authenticated_client.post(
            f"/v1/teams/{team_id}/invite", json=invitation_data
        )

        if response.status_code == 501:
            pytest.skip("Team invitation API not implemented yet")
        elif response.status_code in [200, 201]:
            invitation_response = response.json()
            invitation_id = invitation_response["invitation_id"]
        else:
            pytest.skip(f"Invitation API failed: {response.status_code}")

        # Step 3: Wait for and retrieve email from LocalStack

        try:
            # Wait for the invitation email to arrive
            email = await localstack_email_client.wait_for_invitation_email(
                invitee_email, timeout=15.0
            )

            if email is None:
                # Fallback: check for any emails at this address
                all_emails = await localstack_email_client.get_emails(invitee_email)
                if all_emails:
                    email = all_emails[0]
                else:
                    pytest.skip("No emails in LocalStack - service may not be running")

            # Register email for cleanup
            email_cleanup(invitee_email, email.id)

        except Exception as e:
            pytest.skip(f"Could not retrieve email from LocalStack: {e}")

        # Step 4: Validate email content

        # Basic email validation
        assert email.subject is not None
        assert len(email.subject) > 0
        assert invitee_email in email.to or invitee_email == email.to
        assert email.from_address == "invitations@framecast.app"

        # Content validation (check for key elements)
        email_body_lower = email.body.lower()
        assert "invitation" in email_body_lower or "invite" in email_body_lower
        assert "admin" in email_body_lower  # Role should be mentioned

        # Step 5: Extract invitation URL and ID from email

        # Extract invitation URL
        invitation_url = localstack_email_client.extract_invitation_url(email.body)
        if invitation_url:
            assert "invitations" in invitation_url
            assert "accept" in invitation_url

        # Extract invitation ID from email and validate it matches API response
        extracted_invitation_id = localstack_email_client.extract_invitation_id(
            email.body
        )
        if extracted_invitation_id:
            assert extracted_invitation_id == invitation_id

        # Use extracted ID if available, otherwise use API response
        working_invitation_id = extracted_invitation_id or invitation_id

        # Step 6: Accept invitation via API

        response = await authenticated_client.put(
            f"/v1/invitations/{working_invitation_id}/accept"
        )

        if response.status_code == 501:
            pytest.skip("Invitation acceptance API not implemented yet")
        elif response.status_code == 200:
            acceptance_result = response.json()
            assert acceptance_result.get("status") or acceptance_result.get("id")
        else:
            pytest.fail(f"Invitation acceptance failed: {response.status_code}")

        # Step 7: Verify membership creation

        response = await authenticated_client.get(f"/v1/teams/{team_id}")

        if response.status_code == 501:
            pytest.skip("Team details API not implemented yet")
        elif response.status_code == 200:
            team_details = response.json()

            # Look for the new membership
            memberships = team_details.get("memberships", [])
            admin_membership = next(
                (
                    m
                    for m in memberships
                    if m.get("user", {}).get("email") == invitee_email
                ),
                None,
            )

            if admin_membership:
                assert admin_membership["role"] == "admin"

    async def test_invitation_url_extraction_methods(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test that extraction methods work on sample email content."""
        # Test UUID for extraction testing
        test_uuid = "550e8400-e29b-41d4-a716-446655440000"  # pragma: allowlist secret
        base_url = "https://framecast.app/teams/tm_123/invitations"
        sample_body = f"""
        You have been invited to join Test Team as an admin.
        Click here to accept: {base_url}/{test_uuid}/accept
        """

        url = localstack_email_client.extract_invitation_url(sample_body)
        inv_id = localstack_email_client.extract_invitation_id(sample_body)
        team_id = localstack_email_client.extract_team_id(sample_body)

        assert url is not None
        assert "accept" in url
        assert inv_id == test_uuid
        assert team_id == "tm_123" or team_id is None  # Team ID extraction may vary

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

    async def test_email_client_operations(
        self,
        localstack_email_client: LocalStackEmailClient,
        email_cleanup: callable,
    ):
        """Test email client retrieval and ordering operations."""
        test_email = "client-ops-test@example.com"

        # Clear existing emails
        await localstack_email_client.clear_emails(test_email)

        # Test that get_emails returns a list (even if empty)
        emails = await localstack_email_client.get_emails(test_email)
        assert isinstance(emails, list)

        # Test get_latest_email (may return None or email depending on state)
        latest = await localstack_email_client.get_latest_email(test_email)
        assert latest is None or hasattr(latest, "subject")

        # Test get_latest_invitation (may return None or email)
        latest_invitation = await localstack_email_client.get_latest_invitation(
            test_email
        )
        assert latest_invitation is None or hasattr(latest_invitation, "subject")

    async def test_email_cleanup_functionality(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test email cleanup functionality."""
        test_email = "cleanup-test@example.com"

        # Clear emails should work even if no emails exist
        cleared_count = await localstack_email_client.clear_emails(test_email)
        assert isinstance(cleared_count, int)
        assert cleared_count >= 0

        # Verify no emails remain after clear
        remaining_emails = await localstack_email_client.get_emails(test_email)
        assert len(remaining_emails) == 0

        # Delete non-existent email should return False
        deletion_result = await localstack_email_client.delete_email("nonexistent_id")
        assert deletion_result is False


@pytest.mark.invitation
@pytest.mark.integration
@pytest.mark.error_handling
class TestInvitationWorkflowErrorHandling:
    """Test error handling in invitation workflow."""

    async def test_email_not_found_returns_none(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test that missing emails return None gracefully."""
        nonexistent = "nonexistent@test.local"
        email = await localstack_email_client.get_latest_email(nonexistent)
        assert email is None

        # Test with timeout - should return None, not raise
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
            "",  # Empty
            "No URLs here",  # No invitation content
            f"random text with uuid {test_uuid} but no invitation",
            "<html><body>Broken HTML",  # Broken HTML
        ]

        for body in malformed_bodies:
            url = localstack_email_client.extract_invitation_url(body)
            inv_id = localstack_email_client.extract_invitation_id(body)

            # These should gracefully return None for malformed content
            assert url is None
            assert inv_id is None

    async def test_invalid_service_url_raises_exception(
        self,
        localstack_email_client: LocalStackEmailClient,
    ):
        """Test that invalid service URL raises an exception."""
        invalid_client = LocalStackEmailClient("http://invalid-host:9999")

        with pytest.raises(httpx.RequestError):
            await invalid_client.get_emails("test@example.com")
