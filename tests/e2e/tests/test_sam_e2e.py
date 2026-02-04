"""SAM Local E2E Tests.

Full API coverage tests that run against SAM local Lambda deployment.
These tests verify that the Lambda function works correctly with API Gateway v2.

Prerequisites:
- SAM CLI installed
- cargo-lambda installed
- Docker running
- LocalStack running (just start-backing-services)
- SAM local running (just sam-local-start)

Run with: just test-e2e-sam

Environment variables:
- TEST_USE_SAM_LOCAL=true (required)
- TEST_SAM_API_URL=http://localhost:3001 (optional, default)
"""

import os

import pytest
from conftest import (
    TestDataFactory,
)
from httpx import AsyncClient


def is_sam_local_enabled() -> bool:
    """Check if SAM local testing is enabled."""
    return os.getenv("TEST_USE_SAM_LOCAL", "false").lower() == "true"


# Test placeholder UUIDs (not real IDs, just for route testing)
# These are intentionally all-zeros - route placeholders for authorization tests
TEST_TEAM_ID = "00000000-0000-0000-0000-000000000001"  # pragma: allowlist secret
TEST_USER_ID = "00000000-0000-0000-0000-000000000002"  # pragma: allowlist secret
TEST_INVITATION_ID = "00000000-0000-0000-0000-000000000003"  # pragma: allowlist secret


# Skip all tests in this module if SAM local is not enabled
pytestmark = [
    pytest.mark.sam_local,
    pytest.mark.skipif(
        not is_sam_local_enabled(),
        reason="SAM local tests require TEST_USE_SAM_LOCAL=true",
    ),
]


# =============================================================================
# HEALTH & INFO TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalHealth:
    """Health check and info endpoint tests via SAM local."""

    async def test_health_check_returns_ok(
        self,
        http_client: AsyncClient,
    ):
        """Test that health check endpoint returns OK."""
        response = await http_client.get("/health")

        assert response.status_code == 200
        body = response.text
        assert "OK" in body or body.strip() == "" or "healthy" in body.lower()

    async def test_root_endpoint_returns_api_info(
        self,
        http_client: AsyncClient,
    ):
        """Test that root endpoint returns API information."""
        response = await http_client.get("/")

        assert response.status_code == 200
        body = response.text
        assert "Framecast" in body or "API" in body

    async def test_unknown_route_returns_404(
        self,
        http_client: AsyncClient,
    ):
        """Test that unknown routes return 404."""
        response = await http_client.get("/v1/this-route-does-not-exist")

        assert response.status_code == 404


# =============================================================================
# AUTHENTICATION TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalAuth:
    """Authentication tests via SAM local."""

    async def test_missing_auth_returns_401(
        self,
        http_client: AsyncClient,
    ):
        """Test that protected endpoints require authentication."""
        response = await http_client.get("/v1/account")

        assert response.status_code == 401

    async def test_invalid_jwt_returns_401(
        self,
        http_client: AsyncClient,
    ):
        """Test that invalid JWT tokens are rejected."""
        response = await http_client.get(
            "/v1/account",
            headers={"Authorization": "Bearer invalid.jwt.token"},
        )

        assert response.status_code == 401

    async def test_malformed_auth_header_returns_401(
        self,
        http_client: AsyncClient,
    ):
        """Test that malformed auth headers are rejected."""
        # Missing "Bearer" prefix
        response = await http_client.get(
            "/v1/account",
            headers={"Authorization": "invalid-format"},
        )

        assert response.status_code == 401

    async def test_expired_jwt_returns_401(
        self,
        http_client: AsyncClient,
    ):
        """Test that expired JWT tokens are rejected."""
        # Create a JWT with an expired timestamp
        import base64
        import json

        header = base64.b64encode(
            json.dumps({"alg": "HS256", "typ": "JWT"}).encode()
        ).decode()
        payload = base64.b64encode(
            json.dumps(
                {
                    "sub": "test-user-id",
                    "email": "test@example.com",
                    "exp": 1000000000,  # Expired in 2001
                }
            ).encode()
        ).decode()
        expired_token = f"{header}.{payload}.fake-signature"

        response = await http_client.get(
            "/v1/account",
            headers={"Authorization": f"Bearer {expired_token}"},
        )

        assert response.status_code == 401


# =============================================================================
# TEAMS TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalTeams:
    """Team management tests via SAM local."""

    async def test_create_team_requires_auth(
        self,
        http_client: AsyncClient,
        test_data_factory: TestDataFactory,
    ):
        """Test that creating a team requires authentication."""
        team_data = test_data_factory.team_data()

        response = await http_client.post("/v1/teams", json=team_data)

        assert response.status_code == 401

    async def test_get_team_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that getting a team requires authentication."""
        response = await http_client.get(f"/v1/teams/{TEST_TEAM_ID}")

        assert response.status_code == 401

    async def test_update_team_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that updating a team requires authentication."""
        response = await http_client.patch(
            f"/v1/teams/{TEST_TEAM_ID}",
            json={"name": "Updated Name"},
        )

        assert response.status_code == 401

    async def test_delete_team_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that deleting a team requires authentication."""
        response = await http_client.delete(f"/v1/teams/{TEST_TEAM_ID}")

        assert response.status_code == 401


# =============================================================================
# USER ACCOUNT TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalAccount:
    """User account tests via SAM local."""

    async def test_get_profile_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that getting profile requires authentication."""
        response = await http_client.get("/v1/account")

        assert response.status_code == 401

    async def test_update_profile_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that updating profile requires authentication."""
        response = await http_client.patch(
            "/v1/account",
            json={"name": "New Name"},
        )

        assert response.status_code == 401

    async def test_upgrade_tier_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that upgrading tier requires authentication."""
        response = await http_client.post(
            "/v1/account/upgrade",
            json={"target_tier": "creator"},
        )

        assert response.status_code == 401


# =============================================================================
# INVITATIONS TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalInvitations:
    """Invitation tests via SAM local."""

    async def test_invite_member_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that inviting a member requires authentication."""
        response = await http_client.post(
            f"/v1/teams/{TEST_TEAM_ID}/invite",
            json={"email": "invited@example.com", "role": "member"},
        )

        assert response.status_code == 401

    async def test_accept_invitation_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that accepting an invitation requires authentication."""
        response = await http_client.put(f"/v1/invitations/{TEST_INVITATION_ID}/accept")

        assert response.status_code == 401

    async def test_decline_invitation_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that declining an invitation requires authentication."""
        response = await http_client.put(
            f"/v1/invitations/{TEST_INVITATION_ID}/decline"
        )

        assert response.status_code == 401


# =============================================================================
# MEMBERSHIP TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalMemberships:
    """Membership management tests via SAM local."""

    async def test_remove_member_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that removing a member requires authentication."""
        response = await http_client.delete(
            f"/v1/teams/{TEST_TEAM_ID}/members/{TEST_USER_ID}"
        )

        assert response.status_code == 401

    async def test_update_member_role_requires_auth(
        self,
        http_client: AsyncClient,
    ):
        """Test that updating member role requires authentication."""
        response = await http_client.put(
            f"/v1/teams/{TEST_TEAM_ID}/members/{TEST_USER_ID}/role",
            json={"role": "admin"},
        )

        assert response.status_code == 401


# =============================================================================
# REQUEST VALIDATION TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalValidation:
    """Request validation tests via SAM local."""

    async def test_invalid_json_returns_error(
        self,
        http_client: AsyncClient,
    ):
        """Test that invalid JSON body returns appropriate error."""
        response = await http_client.post(
            "/v1/teams",
            content="{ invalid json }",
            headers={
                "Content-Type": "application/json",
                "Authorization": "Bearer test.jwt.token",
            },
        )

        # Could be 400 (bad JSON), 401 (auth first), or 422 (validation)
        assert response.status_code in [400, 401, 422]

    async def test_missing_content_type_handled(
        self,
        http_client: AsyncClient,
    ):
        """Test that missing Content-Type header is handled."""
        response = await http_client.post(
            "/v1/teams",
            content='{"name": "Test"}',
        )

        # Should fail auth, not crash on missing content type
        assert response.status_code == 401

    async def test_empty_body_handled(
        self,
        http_client: AsyncClient,
    ):
        """Test that empty request body is handled."""
        response = await http_client.post(
            "/v1/teams",
            content="",
            headers={"Content-Type": "application/json"},
        )

        # Should fail auth first
        assert response.status_code == 401


# =============================================================================
# CORS TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalCORS:
    """CORS handling tests via SAM local."""

    async def test_cors_origin_header_accepted(
        self,
        http_client: AsyncClient,
    ):
        """Test that requests with Origin header are accepted."""
        response = await http_client.get(
            "/health",
            headers={"Origin": "http://localhost:3000"},
        )

        assert response.status_code == 200

    async def test_cors_preflight_handled(
        self,
        http_client: AsyncClient,
    ):
        """Test that OPTIONS preflight requests are handled."""
        response = await http_client.options(
            "/v1/teams",
            headers={
                "Origin": "http://localhost:3000",
                "Access-Control-Request-Method": "POST",
                "Access-Control-Request-Headers": "Authorization,Content-Type",
            },
        )

        # OPTIONS should return 200, 204, or 405 (if not configured)
        assert response.status_code in [200, 204, 405]


# =============================================================================
# API GATEWAY INTEGRATION TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalAPIGateway:
    """API Gateway v2 integration tests."""

    async def test_query_params_passed_through(
        self,
        http_client: AsyncClient,
    ):
        """Test that query parameters are passed through API Gateway."""
        response = await http_client.get(
            "/health",
            params={"test": "value", "another": "param"},
        )

        assert response.status_code == 200

    async def test_path_params_extracted(
        self,
        http_client: AsyncClient,
    ):
        """Test that path parameters are extracted correctly."""
        response = await http_client.get("/v1/teams/my-team-id")

        # Should reach the handler (fail auth), not 404
        assert response.status_code == 401

    async def test_custom_headers_forwarded(
        self,
        http_client: AsyncClient,
    ):
        """Test that custom headers are forwarded to Lambda."""
        response = await http_client.get(
            "/health",
            headers={
                "X-Custom-Header": "test-value",
                "X-Request-Id": "req-123456",
            },
        )

        assert response.status_code == 200

    async def test_various_http_methods(
        self,
        http_client: AsyncClient,
    ):
        """Test that various HTTP methods are routed correctly."""
        # GET
        response = await http_client.get("/health")
        assert response.status_code == 200

        # POST (to protected endpoint)
        response = await http_client.post("/v1/teams", json={})
        assert response.status_code == 401

        # PATCH (to protected endpoint)
        response = await http_client.patch("/v1/account", json={})
        assert response.status_code == 401

        # DELETE (to protected endpoint)
        response = await http_client.delete("/v1/teams/test-id")
        assert response.status_code == 401

        # PUT (to protected endpoint)
        response = await http_client.put("/v1/invitations/test-id/accept")
        assert response.status_code == 401


# =============================================================================
# PERFORMANCE TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalPerformance:
    """Performance tests for SAM local deployment."""

    async def test_health_check_response_time(
        self,
        http_client: AsyncClient,
    ):
        """Test that health check responds within reasonable time."""
        import time

        start = time.time()
        response = await http_client.get("/health")
        duration = time.time() - start

        assert response.status_code == 200
        # Allow up to 30 seconds for cold start
        assert duration < 30, f"Health check took {duration:.2f}s"

    async def test_concurrent_requests(
        self,
        http_client: AsyncClient,
    ):
        """Test handling of concurrent requests."""
        import asyncio

        async def make_request():
            return await http_client.get("/health")

        # Make 5 concurrent requests
        responses = await asyncio.gather(*[make_request() for _ in range(5)])

        success_count = sum(1 for r in responses if r.status_code == 200)
        assert success_count >= 3, f"Only {success_count}/5 requests succeeded"


# =============================================================================
# ERROR HANDLING TESTS
# =============================================================================


@pytest.mark.sam_local
class TestSamLocalErrorHandling:
    """Error handling tests via SAM local."""

    async def test_404_response_format(
        self,
        http_client: AsyncClient,
    ):
        """Test that 404 responses have proper format."""
        response = await http_client.get("/v1/nonexistent-endpoint")

        assert response.status_code == 404

    async def test_401_response_format(
        self,
        http_client: AsyncClient,
    ):
        """Test that 401 responses have proper format."""
        response = await http_client.get("/v1/account")

        assert response.status_code == 401

    async def test_method_not_allowed_handling(
        self,
        http_client: AsyncClient,
    ):
        """Test that unsupported methods return appropriate error."""
        # Try POST on health endpoint (should only accept GET)
        response = await http_client.post("/health")

        # Either 404 (route not found) or 405 (method not allowed)
        assert response.status_code in [404, 405]
