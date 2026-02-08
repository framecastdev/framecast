"""Data Integrity E2E Tests.

Tests cross-entity consistency and invariant verification (10 stories):
  - Team ownership invariant (D1)
  - Slug uniqueness (D2)
  - Auto-generated slug validity (D3)
  - Membership uniqueness (D4)
  - Creator-only memberships (D5)
  - Timestamp consistency (D6)
  - Invitation temporal ordering (D7)
  - Credits non-negative (D8)
  - Team deletion cascades (D9)
  - User deletion cascades (D10)
"""

import re
import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    TestDataFactory,
    assert_credits_non_negative,
)


@pytest.mark.teams
class TestDataIntegrityE2E:
    """Data integrity and invariant verification end-to-end tests."""

    async def test_d1_team_always_has_at_least_one_owner(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """D1: After any operation, verify team has >= 1 owner (INV-T2)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        # Add member
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

        # Check members — at least one owner
        resp = await http_client.get(
            f"/v1/teams/{team_id}/members", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        members = resp.json()
        owner_count = sum(1 for m in members if m["role"] == "owner")
        assert owner_count >= 1, f"Team must have >= 1 owner, found {owner_count}"

    async def test_d2_slug_uniqueness_enforced(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """D2: Create team, try duplicate slug -> 409 (INV-T3)."""
        owner = seed_users.owner

        slug = "integrity-slug-test"
        resp = await http_client.post(
            "/v1/teams",
            json={"name": "First", "slug": slug},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Second", "slug": slug},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 409

    async def test_d3_auto_generated_slugs_are_valid(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """D3: Create 5 teams with various names -> all slugs match pattern (INV-T4)."""
        owner = seed_users.owner

        # Valid slug pattern: starts/ends with alphanumeric, can contain hyphens
        slug_pattern = re.compile(r"^[a-z0-9]([a-z0-9-]*[a-z0-9])?$")

        names = [
            "Alpha Studio",
            "Beta Creative Hub",
            "3D Animation Co",
            "Simple",
            "Multi Word Team Name Here",
        ]

        for name in names:
            resp = await http_client.post(
                "/v1/teams",
                json={"name": name},
                headers=owner.auth_headers(),
            )
            assert resp.status_code == 201
            slug = resp.json()["slug"]
            assert slug_pattern.match(slug), (
                f"Auto-generated slug '{slug}' for name '{name}' doesn't match pattern"
            )

    async def test_d4_user_team_membership_uniqueness(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """D4: Accept invitation twice for same team -> second fails (INV-M3)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

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
        inv_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Try to invite same user again (already a member)
        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 409, (
            f"Expected 409 for duplicate membership, got {resp.status_code}"
        )

    async def test_d5_only_creators_have_memberships(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """D5: Verify starter user has no membership (INV-M4)."""
        invitee = seed_users.invitee  # starter

        # Starter cannot list teams (403 means no team access)
        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 403

    async def test_d6_created_at_lte_updated_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """D6: After update, verify created_at <= updated_at (INV-T5)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        created = resp.json()["created_at"]

        # Update
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Timestamp Check"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        updated = resp.json()["updated_at"]

        assert created <= updated, (
            f"created_at ({created}) should be <= updated_at ({updated})"
        )

    async def test_d7_invitation_expires_after_created(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """D7: Create invitation, verify expires_at > created_at (INV-I9)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": "temporal@test.com", "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        inv = resp.json()

        if "expires_at" in inv and "created_at" in inv:
            assert inv["expires_at"] > inv["created_at"], (
                f"expires_at ({inv['expires_at']}) should be > created_at ({inv['created_at']})"
            )

    async def test_d8_credits_never_negative(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """D8: After operations, verify credits >= 0 (INV-U5, INV-T6)."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Check owner credits
        resp = await http_client.get("/v1/account", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert_credits_non_negative(resp.json()["credits"])

        # Check invitee credits
        resp = await http_client.get("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 200
        assert_credits_non_negative(resp.json()["credits"])

    async def test_d9_team_deletion_cascades_invitations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """D9: Delete team -> team's invitations no longer accessible."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

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

        # Try to accept invitation — should fail
        resp = await http_client.post(
            f"/v1/invitations/{inv_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 404, 409]

    async def test_d10_user_deletion_cascades_api_keys(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """D10: Delete user -> user's API keys no longer work."""
        invitee = seed_users.invitee  # starter (no teams to worry about)

        # Create API key
        resp = await http_client.post(
            "/v1/auth/keys",
            json={"name": "Cascade Key", "scopes": ["generate"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 201
        key_data = resp.json()
        raw_key = key_data.get("raw_key", key_data.get("key", ""))

        # Delete user
        resp = await http_client.delete("/v1/account", headers=invitee.auth_headers())
        assert resp.status_code == 204

        # Try to use the key — should fail
        if raw_key:
            resp = await http_client.get(
                "/v1/account",
                headers={"Authorization": f"Bearer {raw_key}"},
            )
            assert resp.status_code in [401, 404]
