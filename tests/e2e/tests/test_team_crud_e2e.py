"""Team CRUD Operations E2E Tests.

Tests team create/read/update/delete operations (22 stories):
  - Happy path (T1-T9)
  - Validation & edge cases (T10-T18)
  - Permission & security (T19-T22)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers, TestDataFactory  # noqa: E402


@pytest.mark.teams
class TestTeamCrudE2E:
    """Team CRUD end-to-end tests."""

    # -----------------------------------------------------------------------
    # Happy Path (T1-T9)
    # -----------------------------------------------------------------------

    async def test_t1_create_team_with_name_only(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T1: POST /v1/teams with name; verify 201, auto-slug, owner membership."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "My Test Team"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Team creation failed: {resp.status_code} {resp.text}"
        )
        team = resp.json()
        assert team["name"] == "My Test Team"
        assert "slug" in team
        assert len(team["slug"]) > 0
        assert team["user_role"] == "owner"

    async def test_t2_create_team_with_custom_slug(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T2: POST /v1/teams with name + slug; verify slug matches."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Custom Slug Team", "slug": "custom-slug-team"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Create with slug failed: {resp.status_code} {resp.text}"
        )
        assert resp.json()["slug"] == "custom-slug-team"

    async def test_t3_create_team_then_update_settings(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T3: Create team, then PATCH settings; verify settings persisted."""
        owner = seed_users.owner

        # Create team
        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Settings Team"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Create team failed: {resp.status_code} {resp.text}"
        )
        team_id = resp.json()["id"]

        # Update settings via PATCH
        settings_data = {
            "default_resolution": "1920x1080",
            "webhook_url": "https://example.com/hook",
        }
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"settings": settings_data},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Update settings failed: {resp.status_code} {resp.text}"
        )
        assert resp.json()["settings"] == settings_data

    async def test_t4_list_teams_shows_all_memberships(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T4: Creator in 3 teams -> GET /v1/teams returns all 3 with user_role."""
        owner = seed_users.owner

        team_ids = []
        for _ in range(3):
            resp = await http_client.post(
                "/v1/teams",
                json=test_data_factory.team_data(),
                headers=owner.auth_headers(),
            )
            assert resp.status_code == 201
            team_ids.append(resp.json()["id"])

        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        assert resp.status_code == 200
        teams = resp.json()
        assert len(teams) >= 3
        returned_ids = {t["id"] for t in teams}
        for tid in team_ids:
            assert tid in returned_ids
        for t in teams:
            assert "user_role" in t

    async def test_t5_get_team_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T5: GET /v1/teams/:id returns team with all fields + user_role."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        team = resp.json()
        assert team["id"] == team_id
        assert "name" in team
        assert "slug" in team
        assert "user_role" in team
        assert "created_at" in team
        assert "updated_at" in team

    async def test_t6_update_team_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T6: PATCH /v1/teams/:id with new name; verify name changed."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        new_name = "Renamed Team"
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": new_name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Update team name failed: {resp.status_code} {resp.text}"
        )
        assert resp.json()["name"] == new_name

    async def test_t7_update_team_settings(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T7: PATCH /v1/teams/:id with new settings JSON; verify settings replaced."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        new_settings = {"resolution": "4k", "fps": 60}
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"settings": new_settings},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json()["settings"] == new_settings

    async def test_t8_delete_team_sole_member(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T8: DELETE /v1/teams/:id as sole owner -> 204."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.delete(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204, (
            f"Delete team failed: {resp.status_code} {resp.text}"
        )

    async def test_t9_list_teams_ordered_by_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T9: Create teams "Zebra", "Alpha"; verify list returns Alpha first."""
        owner = seed_users.owner

        for name in ["Zebra Studio", "Alpha Studio"]:
            resp = await http_client.post(
                "/v1/teams",
                json={"name": name},
                headers=owner.auth_headers(),
            )
            assert resp.status_code == 201

        resp = await http_client.get("/v1/teams", headers=owner.auth_headers())
        assert resp.status_code == 200
        teams = resp.json()
        team_names_returned = [t["name"] for t in teams]
        # Verify alphabetical ordering (case-insensitive)
        assert team_names_returned == sorted(team_names_returned, key=str.lower)

    # -----------------------------------------------------------------------
    # Validation & Edge Cases (T10-T18)
    # -----------------------------------------------------------------------

    async def test_t10_create_with_empty_name_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T10: POST /v1/teams with name="" -> 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": ""},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for empty name, got {resp.status_code} {resp.text}"
        )

    async def test_t11_create_with_too_long_name_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T11: POST /v1/teams with name > 100 chars -> 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "A" * 101},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for too-long name, got {resp.status_code} {resp.text}"
        )

    async def test_t12_create_with_invalid_slug_uppercase(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T12: POST /v1/teams with slug="UPPER_CASE" -> 400 (INV-T4)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Test", "slug": "UPPER_CASE"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for uppercase slug, got {resp.status_code} {resp.text}"
        )

    async def test_t13_create_with_slug_leading_hyphen(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T13: POST /v1/teams with slug="-bad" -> 400 (INV-T4)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Test", "slug": "-bad"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for leading-hyphen slug, got {resp.status_code} {resp.text}"
        )

    async def test_t14_create_with_slug_trailing_hyphen(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T14: POST /v1/teams with slug="bad-" -> 400 (INV-T4)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Test", "slug": "bad-"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for trailing-hyphen slug, got {resp.status_code} {resp.text}"
        )

    async def test_t15_create_with_duplicate_slug_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T15: Two teams with same slug -> 409 (INV-T3)."""
        owner = seed_users.owner

        slug = "unique-slug-test"
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
        assert resp.status_code == 409, (
            f"Expected 409 for duplicate slug, got {resp.status_code} {resp.text}"
        )

    async def test_t16_delete_team_with_other_members_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T16: Owner tries delete while non-owner members exist -> 400/409."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team and add member
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        invitation_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Try to delete team with members
        resp = await http_client.delete(
            f"/v1/teams/{team_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 409, (
            f"Expected 409 for deleting team with members, got {resp.status_code} {resp.text}"
        )

    async def test_t17_get_nonexistent_team_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T17: GET /v1/teams/{random-uuid} -> 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/teams/{fake_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404, (
            f"Expected 404 for nonexistent team, got {resp.status_code}"
        )

    async def test_t18_update_nonexistent_team_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T18: PATCH /v1/teams/{random-uuid} -> 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.patch(
            f"/v1/teams/{fake_id}",
            json={"name": "Nope"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404, (
            f"Expected 404 for nonexistent team update, got {resp.status_code}"
        )

    # -----------------------------------------------------------------------
    # Permission & Security (T19-T22)
    # -----------------------------------------------------------------------

    async def test_t19_starter_cannot_create_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """T19: Starter POST /v1/teams -> 403."""
        invitee = seed_users.invitee  # starter

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Starter Team"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403, (
            f"Expected 403 for starter creating team, got {resp.status_code} {resp.text}"
        )

    async def test_t20_non_member_cannot_view_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T20: Creator not in team -> GET /v1/teams/:id -> 403."""
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

        # Upgrade invitee to creator
        resp = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [200, 409]

        # Invitee tries to view owner's team
        resp = await http_client.get(
            f"/v1/teams/{team_id}", headers=invitee.auth_headers()
        )
        assert resp.status_code == 403, (
            f"Expected 403 for non-member view, got {resp.status_code}"
        )

    async def test_t21_member_cannot_update_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T21: Member role tries PATCH /v1/teams/:id -> 403."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team and add invitee as member
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "member"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        invitation_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Member tries to update team
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Hacked"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 403, (
            f"Expected 403 for member updating team, got {resp.status_code} {resp.text}"
        )

    async def test_t22_admin_can_update_team(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """T22: Admin role PATCH /v1/teams/:id -> 200."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        # Create team and add invitee as admin
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/teams/{team_id}/invitations",
            json={"email": invitee.email, "role": "admin"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        invitation_id = resp.json()["id"]

        resp = await http_client.post(
            f"/v1/invitations/{invitation_id}/accept",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200

        # Admin updates team
        resp = await http_client.patch(
            f"/v1/teams/{team_id}",
            json={"name": "Admin Updated"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 200, (
            f"Expected 200 for admin updating team, got {resp.status_code} {resp.text}"
        )
        assert resp.json()["name"] == "Admin Updated"

    # -----------------------------------------------------------------------
    # Validation: Parametrized Tests
    # -----------------------------------------------------------------------

    @pytest.mark.parametrize(
        "name",
        [
            "Simple Team",
            "Team !@#$%",
            "X" * 100,
            "\u65e5\u672c\u8a9e\u30c1\u30fc\u30e0",
        ],
        ids=["ascii", "special-chars", "max-length", "unicode"],
    )
    async def test_valid_team_name_never_returns_500(
        self,
        name: str,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """Any valid team name either succeeds (201) or validation error, never 500."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [201, 400, 409], (
            f"Unexpected status {resp.status_code} for name={name!r}: {resp.text}"
        )

    @pytest.mark.parametrize(
        "name",
        ["", "A" * 101],
        ids=["empty", "too-long"],
    )
    async def test_invalid_team_name_always_rejected(
        self,
        name: str,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """Invalid team names always return 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for invalid name={name!r}, got {resp.status_code}"
        )

    @pytest.mark.parametrize(
        "slug",
        ["---", "UPPERCASE", "-leading", "trailing-"],
        ids=["all-hyphens", "uppercase", "leading-hyphen", "trailing-hyphen"],
    )
    async def test_invalid_slug_always_rejected(
        self,
        slug: str,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """Invalid slugs always return 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Valid Name", "slug": slug},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for invalid slug={slug!r}, got {resp.status_code}"
        )
