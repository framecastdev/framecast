"""Invitation Email Verification E2E Tests.

Tests invitation email content via LocalStack SES (8 stories):
  - Email content verification (IE1-IE4)
  - Resend and re-invite emails (IE5-IE6)
  - No-email scenarios (IE7-IE8)

Requires LocalStack SES — tagged @pytest.mark.real_services
"""

import asyncio
import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402
from utils.localstack_email import LocalStackEmailClient  # noqa: E402


@pytest.mark.invitation
@pytest.mark.real_services
class TestInvitationEmailE2E:
    """Invitation email verification end-to-end tests."""

    # -----------------------------------------------------------------------
    # Helper
    # -----------------------------------------------------------------------

    async def _create_team_and_invite(
        self,
        http_client: httpx.AsyncClient,
        owner,
        invitee_email: str,
        test_data_factory: TestDataFactory,
        team_name: str = None,
        role: str = "member",
    ) -> tuple[str, str, str]:
        """Create team, invite email. Returns (team_id, invitation_id, team_name)."""
        data = test_data_factory.team_data()
        if team_name:
            data["name"] = team_name

        resp = await http_client.post(
            "/v1/teams", json=data, headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        team = resp.json()
        team_id = team["id"]
        actual_name = team["name"]

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee_email, "role": role},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        return team_id, invitation_id, actual_name

    async def _wait_for_email_with_team_name(
        self,
        localstack_email_client: LocalStackEmailClient,
        email_address: str,
        team_name: str,
        timeout: float = 15,
    ):
        """Wait for an email containing the team name."""
        start = asyncio.get_event_loop().time()
        while (asyncio.get_event_loop().time() - start) < timeout:
            emails = await localstack_email_client.get_emails(email_address)
            for e in emails:
                if team_name in (e.subject or "") or team_name in (e.body or ""):
                    return e
            await asyncio.sleep(0.5)
        return None

    # -----------------------------------------------------------------------
    # Email Content (IE1-IE4)
    # -----------------------------------------------------------------------

    async def test_ie1_email_contains_team_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE1: Invitation email subject/body includes team name."""
        owner = seed_users.owner
        invitee = seed_users.invitee
        team_name = "Email Content Test Studio"

        _, _, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name=team_name,
        )

        email = await self._wait_for_email_with_team_name(
            localstack_email_client, invitee.email, team_name
        )
        assert email is not None, f"Email with team name '{team_name}' not found"
        assert team_name in (email.subject or "") or team_name in (email.body or "")

    async def test_ie2_email_contains_inviter_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE2: Invitation email body includes who invited them."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        _, _, team_name = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="Inviter Name Test Studio",
        )

        email = await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )
        assert email is not None, "Invitation email not received"
        # Check for inviter name or email in body
        assert owner.name in (email.body or "") or owner.email in (email.body or ""), (
            f"Inviter info not found in email body: {email.body[:200]}"
        )

    async def test_ie3_email_contains_invitation_url(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE3: Invitation email body includes clickable accept link."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        _, _, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="URL Test Studio",
        )

        email = await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )
        assert email is not None, "Invitation email not received"

        url = localstack_email_client.extract_invitation_url(email.body)
        assert url is not None, (
            f"Email should contain invitation URL. Body: {email.body[:200]}"
        )

    async def test_ie4_email_sent_to_correct_recipient(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE4: Email destination matches invitee email."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        _, _, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="Recipient Test Studio",
        )

        email = await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )
        assert email is not None, "Invitation email not received"
        assert invitee.email in email.to, (
            f"Email should be addressed to {invitee.email}, got {email.to}"
        )

    # -----------------------------------------------------------------------
    # Resend / Re-invite (IE5-IE6)
    # -----------------------------------------------------------------------

    async def test_ie5_resend_triggers_new_email(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE5: After resend, a second email arrives."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id, invitation_id, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="Resend Email Studio",
        )

        # Wait for first email
        email1 = await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )
        assert email1 is not None, "First invitation email not received"

        # Count emails before resend
        emails_before = await localstack_email_client.get_emails(invitee.email)
        count_before = len(emails_before)

        # Resend
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations/{invitation_id}/resend",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Wait for second email
        await asyncio.sleep(2)
        emails_after = await localstack_email_client.get_emails(invitee.email)
        assert len(emails_after) > count_before, (
            f"Expected more emails after resend: before={count_before}, after={len(emails_after)}"
        )

    async def test_ie6_reinvite_sends_new_email(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE6: Replacing invitation (revoke+new) sends fresh email."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id, first_inv_id, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="Reinvite Email Studio",
        )

        # Wait for first email
        await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )

        # Count emails before re-invite
        emails_before = await localstack_email_client.get_emails(invitee.email)
        count_before = len(emails_before)

        # Re-invite (auto-revokes old)
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "admin"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Wait for new email
        await asyncio.sleep(2)
        emails_after = await localstack_email_client.get_emails(invitee.email)
        assert len(emails_after) > count_before, (
            f"Expected new email after re-invite: before={count_before}, after={len(emails_after)}"
        )

    # -----------------------------------------------------------------------
    # No-Email Scenarios (IE7-IE8)
    # -----------------------------------------------------------------------

    async def test_ie7_decline_does_not_trigger_email(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE7: Declining sends no notification email."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id, invitation_id, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="Decline No Email Studio",
        )

        # Wait for initial invitation email
        await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )

        # Count emails for owner (decline should not notify owner)
        owner_emails_before = await localstack_email_client.get_emails(owner.email)
        count_before = len(owner_emails_before)

        # Decline
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/decline",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 204

        # Wait briefly and check no new email to owner
        await asyncio.sleep(2)
        owner_emails_after = await localstack_email_client.get_emails(owner.email)
        assert len(owner_emails_after) == count_before, (
            "Decline should not send notification email to owner"
        )

    async def test_ie8_accept_triggers_no_additional_email(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        localstack_email_client: LocalStackEmailClient,
        test_data_factory: TestDataFactory,
    ):
        """IE8: Accepting does not send email (membership visible in API)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        team_id, invitation_id, _ = await self._create_team_and_invite(
            http_client,
            owner,
            invitee.email,
            test_data_factory,
            team_name="Accept No Email Studio",
        )

        # Wait for initial invitation email
        await localstack_email_client.wait_for_invitation_email(
            invitee.email, timeout=15
        )

        # Count emails before accept
        invitee_emails_before = await localstack_email_client.get_emails(invitee.email)
        count_before = len(invitee_emails_before)

        # Accept
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Wait and check no new email
        await asyncio.sleep(2)
        invitee_emails_after = await localstack_email_client.get_emails(invitee.email)
        assert len(invitee_emails_after) == count_before, (
            "Accept should not send additional email to invitee"
        )
