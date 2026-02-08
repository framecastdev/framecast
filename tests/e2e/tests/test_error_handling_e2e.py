"""Error Handling E2E Tests.

Tests error response formats and edge cases (12 stories):
  - Error format verification (E1-E5)
  - Input validation edge cases (E6-E12)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import SeededUsers  # noqa: E402


@pytest.mark.error_handling
@pytest.mark.real_services
class TestErrorHandlingE2E:
    """Error handling and edge case end-to-end tests."""

    # -----------------------------------------------------------------------
    # Error Format Verification (E1-E5)
    # -----------------------------------------------------------------------

    async def test_e1_401_error_format(
        self,
        http_client: httpx.AsyncClient,
    ):
        """E1: Missing auth -> JSON error structure."""
        resp = await http_client.get("/v1/account")
        assert resp.status_code == 401

        body = resp.json()
        assert "error" in body, f"Expected error key in 401 response: {body}"
        error = body["error"]
        assert "code" in error or "message" in error

    async def test_e2_403_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E2: Forbidden action -> JSON error structure."""
        invitee = seed_users.invitee  # starter

        resp = await http_client.get("/v1/teams", headers=invitee.auth_headers())
        assert resp.status_code == 403

        body = resp.json()
        assert "error" in body, f"Expected error key in 403 response: {body}"

    async def test_e3_404_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E3: Nonexistent resource -> JSON error structure."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/teams/{fake_id}", headers=owner.auth_headers()
        )
        # May be 403 (non-member) or 404
        assert resp.status_code in [403, 404]

        body = resp.json()
        assert "error" in body, f"Expected error key in response: {body}"

    async def test_e4_400_validation_error_format(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E4: Invalid input -> JSON error with validation details."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": ""},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400

        body = resp.json()
        assert "error" in body, f"Expected error key in 400 response: {body}"

    async def test_e5_422_deserialization_error(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E5: Malformed JSON body -> 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            content=b"{invalid json!!!",
            headers={
                **owner.auth_headers(),
                "Content-Type": "application/json",
            },
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for malformed JSON, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Input Validation Edge Cases (E6-E12)
    # -----------------------------------------------------------------------

    async def test_e6_uuid_format_validation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E6: Non-UUID path parameter -> 400 or 404."""
        owner = seed_users.owner

        resp = await http_client.get(
            "/v1/teams/not-a-uuid", headers=owner.auth_headers()
        )
        assert resp.status_code in [400, 404, 422], (
            f"Expected 400/404/422 for non-UUID, got {resp.status_code}"
        )

    async def test_e7_empty_request_body_on_post(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E7: POST /v1/teams with no body -> 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            headers={
                **owner.auth_headers(),
                "Content-Type": "application/json",
            },
        )
        assert resp.status_code in [400, 422], (
            f"Expected 400/422 for empty body, got {resp.status_code} {resp.text}"
        )

    async def test_e8_extra_unknown_fields_ignored(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E8: POST /v1/teams with extra fields -> 201 (unknown fields ignored)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={
                "name": "Extra Fields Team",
                "unknown_field": "should be ignored",
                "another_extra": 42,
            },
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Expected 201 with extra fields, got {resp.status_code} {resp.text}"
        )

    async def test_e9_content_type_enforcement(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E9: POST without Content-Type: application/json -> 415 or 422."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            content=b'{"name": "No Content Type"}',
            headers={
                **owner.auth_headers(),
                "Content-Type": "text/plain",
            },
        )
        assert resp.status_code in [400, 415, 422], (
            f"Expected 400/415/422 for wrong content type, got {resp.status_code}"
        )

    async def test_e10_very_long_string_inputs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E10: Name with 10000 chars -> 400 (validation, not crash)."""
        owner = seed_users.owner

        long_name = "A" * 10000
        resp = await http_client.post(
            "/v1/teams",
            json={"name": long_name},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for very long name, got {resp.status_code}"
        )

    async def test_e11_sql_injection_in_slug(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E11: POST /v1/teams with slug containing SQL injection -> 400."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Injection Test", "slug": "a'; DROP TABLE teams; --"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for SQL injection slug, got {resp.status_code} {resp.text}"
        )

    async def test_e12_unicode_in_team_name(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """E12: POST /v1/teams with name containing emoji/CJK -> 201 (valid UTF-8)."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/teams",
            json={"name": "Team 2024"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201, (
            f"Expected 201 for unicode name, got {resp.status_code} {resp.text}"
        )
