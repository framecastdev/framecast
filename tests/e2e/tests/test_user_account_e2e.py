"""User Account Management E2E Tests.

Tests the user account user journeys (21 stories):
  - Profile retrieval and updates (U1-U8)
  - Validation and edge cases (U9-U14)
  - Permission and security (U15-U17)
  - Invariant enforcement (U18-U21)
"""

import sys
import time
import uuid
from pathlib import Path

# Add parent directory to path for utils imports
sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import jwt  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    TestDataFactory,
    assert_credits_non_negative,
)
from hypothesis import given, settings  # noqa: E402
from strategies import user_names  # noqa: E402


@pytest.mark.auth
@pytest.mark.real_services
class TestUserAccountE2E:
    """User account management end-to-end tests."""

    # -----------------------------------------------------------------------
    # Happy Path (U1-U8)
    # -----------------------------------------------------------------------

    async def test_u1_get_own_profile(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U1: Starter user retrieves their profile; verify all fields."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200, (
            f"Get profile failed: {resp.status_code} {resp.text}"
        )
        account = resp.json()

        # Verify all expected fields are present
        assert account["id"] == owner.user_id
        assert account["email"] == owner.email
        assert "name" in account
        assert "tier" in account
        assert "credits" in account
        assert "ephemeral_storage_bytes" in account
        assert "created_at" in account
        assert "updated_at" in account

    async def test_u2_update_profile_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U2: User updates their name; verify name changed, updated_at bumped."""
        owner = seed_users.owner

        # Get current profile
        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200
        original = resp.json()

        # Update name
        new_name = "Updated Profile Name"
        resp = await http_client.patch(
            "/v1/account",
            json={"name": new_name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Update profile failed: {resp.status_code} {resp.text}"
        )
        updated = resp.json()
        assert updated["name"] == new_name
        assert updated["updated_at"] >= original["updated_at"]

    async def test_u3_update_profile_avatar_url(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U3: User sets a valid avatar URL; verify persisted."""
        owner = seed_users.owner

        avatar = "https://example.com/avatars/test-avatar.png"
        resp = await http_client.patch(
            "/v1/account",
            json={"avatar_url": avatar},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Update avatar failed: {resp.status_code} {resp.text}"
        )
        updated = resp.json()
        assert updated["avatar_url"] == avatar

    async def test_u4_upgrade_starter_to_creator(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U4: Starter user upgrades; verify tier=creator, upgraded_at set, auto-team + membership."""
        invitee = seed_users.invitee  # starts as starter

        # Upgrade
        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Upgrade failed: {resp.status_code} {resp.text}"
        )

        # Verify profile reflects creator
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        account = resp.json()
        assert account["tier"] == "creator"
        assert account["upgraded_at"] is not None

        # Verify auto-team created with owner membership
        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 200
        teams = resp.json()
        assert len(teams) >= 1, "Upgrade should create auto-team"
        assert any(t["user_role"] == "owner" for t in teams)

    async def test_u5_upgrade_is_idempotent(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U5: Creator user calls upgrade again; verify 200 (not error)."""
        owner = seed_users.owner  # already creator

        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=owner.auth_headers(),
        )
        # Should succeed idempotently or return conflict
        assert resp.status_code in [200, 409], (
            f"Idempotent upgrade should be 200 or 409, got {resp.status_code} {resp.text}"
        )

    async def test_u6_delete_starter_account(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U6: Starter user (no teams) deletes their account; verify 204."""
        invitee = seed_users.invitee  # starter, no teams

        resp = await http_client.delete("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 204, (
            f"Delete account failed: {resp.status_code} {resp.text}"
        )

        # Subsequent auth should fail
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code in [401, 404], (
            f"Expected 401/404 after deletion, got {resp.status_code}"
        )

    async def test_u7_delete_creator_account_sole_member(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """U7: Creator sole member of all teams deletes account; teams auto-deleted."""
        owner = seed_users.owner

        # Create a team (owner is sole member)
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201

        # Delete account
        resp = await http_client.delete("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 204, (
            f"Delete creator account failed: {resp.status_code} {resp.text}"
        )

    async def test_u8_profile_reflects_tier_after_invitation_accept(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """U8: Starter accepts invitation -> tier becomes creator in GET /v1/account."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Invite the starter
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        # Accept invitation
        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Verify profile shows creator tier
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["tier"] == "creator"

    # -----------------------------------------------------------------------
    # Validation & Edge Cases (U9-U14)
    # -----------------------------------------------------------------------

    async def test_u9_update_with_empty_name_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U9: PATCH /v1/account with name="" -> 400."""
        owner = seed_users.owner

        resp = await http_client.patch(
            "/v1/account",
            json={"name": ""},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for empty name, got {resp.status_code} {resp.text}"
        )

    async def test_u10_update_with_too_long_name_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U10: PATCH /v1/account with name > 100 chars -> 400."""
        owner = seed_users.owner

        long_name = "A" * 101
        resp = await http_client.patch(
            "/v1/account",
            json={"name": long_name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for too-long name, got {resp.status_code} {resp.text}"
        )

    async def test_u11_update_with_invalid_avatar_url_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U11: PATCH /v1/account with avatar_url="not-a-url" -> 400."""
        owner = seed_users.owner

        resp = await http_client.patch(
            "/v1/account",
            json={"avatar_url": "not-a-url"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for invalid avatar URL, got {resp.status_code} {resp.text}"
        )

    async def test_u12_partial_update_name_only(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U12: PATCH with only name, avatar_url unchanged."""
        owner = seed_users.owner

        # Set avatar first
        avatar = "https://example.com/avatars/partial-test.png"
        resp = await http_client.patch(
            "/v1/account",
            json={"avatar_url": avatar},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200

        # Update only name
        resp = await http_client.patch(
            "/v1/account",
            json={"name": "Partial Update Name"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        updated = resp.json()
        assert updated["name"] == "Partial Update Name"
        assert updated["avatar_url"] == avatar

    async def test_u13_partial_update_avatar_only(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U13: PATCH with only avatar_url, name unchanged."""
        owner = seed_users.owner

        # Get current name
        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200
        current_name = resp.json()["name"]

        # Update only avatar
        new_avatar = "https://example.com/avatars/new-avatar.png"
        resp = await http_client.patch(
            "/v1/account",
            json={"avatar_url": new_avatar},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        updated = resp.json()
        assert updated["avatar_url"] == new_avatar
        assert updated["name"] == current_name

    async def test_u14_invalid_json_body_on_update(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U14: PATCH /v1/account with malformed JSON -> 422."""
        owner = seed_users.owner

        resp = await http_client.patch(
            "/v1/account",
            content=b"not valid json{{{",
            headers={
                **owner.auth_headers(),
                "Content-Type": "application/json",
            },
        )
        assert resp.status_code == 422, (
            f"Expected 422 for malformed JSON, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Permission & Security (U15-U17)
    # -----------------------------------------------------------------------

    async def test_u15_unauthenticated_access_to_profile(
        self,
        http_client: httpx.AsyncClient,
    ):
        """U15: GET /v1/account without auth -> 401."""
        resp = await http_client.get("/v1/account")
        assert resp.status_code == 401, (
            f"Expected 401 for unauthenticated access, got {resp.status_code}"
        )

    async def test_u16_invalid_jwt_token(
        self,
        http_client: httpx.AsyncClient,
    ):
        """U16: GET /v1/account with garbage Bearer token -> 401."""
        resp = await http_client.get(
            "/v1/account",
            headers={"Authorization": "Bearer garbage-token-here"},
        )
        assert resp.status_code == 401, (
            f"Expected 401 for invalid JWT, got {resp.status_code}"
        )

    async def test_u17_expired_jwt_token(
        self,
        http_client: httpx.AsyncClient,
    ):
        """U17: GET /v1/account with expired token -> 401."""
        import os

        secret = os.environ.get("JWT_SECRET", "test-e2e-secret-key-for-ci-only-0")
        expired_token = jwt.encode(
            {
                "sub": str(uuid.uuid4()),
                "email": "expired@test.com",
                "aud": "authenticated",
                "role": "authenticated",
                "iat": int(time.time()) - 7200,
                "exp": int(time.time()) - 3600,  # expired 1 hour ago
            },
            secret,
            algorithm="HS256",
        )
        resp = await http_client.get(
            "/v1/account",
            headers={"Authorization": f"Bearer {expired_token}"},
        )
        assert resp.status_code == 401, (
            f"Expected 401 for expired JWT, got {resp.status_code}"
        )

    # -----------------------------------------------------------------------
    # Invariant Enforcement (U18-U21)
    # -----------------------------------------------------------------------

    async def test_u18_delete_blocked_when_sole_owner_with_members(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """U18: Creator owns team with other members, cannot delete account (INV-T2)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Owner creates team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Invite and accept
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        invitation_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Owner tries to delete account — should be blocked (sole owner with members)
        resp = await http_client.delete("/v1/account", headers=owner.auth_headers())
        assert resp.status_code in [400, 409], (
            f"Expected 400/409 for sole-owner delete, got {resp.status_code} {resp.text}"
        )

    async def test_u19_credits_non_negative_after_upgrade(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U19: After upgrade, verify credits >= 0 (INV-U5)."""
        owner = seed_users.owner

        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert_credits_non_negative(resp.json()["credits"])

    async def test_u20_creator_always_has_upgraded_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U20: Creator has upgraded_at NOT NULL; starter has upgraded_at NULL (INV-U1)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Creator should have upgraded_at
        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["upgraded_at"] is not None

        # Starter should NOT have upgraded_at
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["upgraded_at"] is None

    async def test_u21_starter_has_no_memberships(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """U21: Fresh starter user -> GET /v1/teams returns 403 (INV-U3)."""
        invitee = seed_users.invitee  # starter

        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 403, (
            f"Expected 403 for starter listing teams, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Property-Based Tests
    # -----------------------------------------------------------------------

    @settings(max_examples=30, deadline=None)
    @given(name=user_names)
    async def test_valid_name_never_returns_500(
        self,
        name: str,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """Property: any valid name either succeeds (200) or returns validation error (400), never 500."""
        owner = seed_users.owner

        resp = await http_client.patch(
            "/v1/account",
            json={"name": name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [200, 400, 422], (
            f"Unexpected status {resp.status_code} for name={name!r}: {resp.text}"
        )
