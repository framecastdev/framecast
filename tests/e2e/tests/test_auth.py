"""Authentication and Authorization E2E Tests.

Tests user stories US-001 through US-010:
- Account registration and verification
- Login/logout workflows
- Password reset flows
- OAuth integration
- Session management and JWT validation
- Rate limiting and security measures
"""

import pytest
from conftest import (
    E2EConfig,
    UserPersona,
    assert_credits_non_negative,
    assert_user_tier_valid,
    assert_valid_urn,
)
from httpx import AsyncClient


@pytest.mark.auth
@pytest.mark.real_services
class TestAccountRegistration:
    """US-001: Account registration with email verification."""

    async def test_visitor_can_register_new_account(
        self,
        http_client: AsyncClient,
        test_config: E2EConfig,
        visitor_user: UserPersona,
    ):
        """Test basic account registration flow."""
        registration_data = {
            "email": visitor_user.email,
            "password": "SecurePassword123!",  # pragma: allowlist secret
            "name": visitor_user.name,
            "terms_accepted": True,
        }

        # Attempt registration
        response = await http_client.post(
            "/api/v1/auth/register", json=registration_data
        )

        # Should return 201 Created for successful registration
        assert response.status_code == 201
        data = response.json()

        # Verify response structure
        assert "user" in data
        assert "verification_required" in data
        assert data["verification_required"] is True

        user_data = data["user"]
        assert user_data["email"] == visitor_user.email
        assert user_data["name"] == visitor_user.name
        assert_user_tier_valid(user_data["tier"])
        assert user_data["tier"] == "starter"  # New users start as starter
        assert_valid_urn(user_data["id"], "user")

        # Verify user starts with starter credits
        assert_credits_non_negative(user_data["credits"])
        assert user_data["credits"] > 0  # Should have welcome credits

        # Should not include password in response
        assert "password" not in user_data

    async def test_registration_duplicate_email_rejected(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test that duplicate email registration is rejected."""
        registration_data = {
            "email": starter_user.email,  # Already exists
            "password": "AnotherPassword456!",  # pragma: allowlist secret
            "name": "Different Name",
            "terms_accepted": True,
        }

        response = await http_client.post(
            "/api/v1/auth/register", json=registration_data
        )

        # Should return 409 Conflict
        assert response.status_code == 409
        error = response.json()["error"]
        assert error["code"] == "CONFLICT"
        assert "email" in error["message"].lower()

    async def test_registration_invalid_email_rejected(self, http_client: AsyncClient):
        """Test that invalid email formats are rejected."""
        invalid_emails = [
            "not-an-email",
            "@example.com",
            "user@",
            "user@.com",
            "",
        ]

        for invalid_email in invalid_emails:
            registration_data = {
                "email": invalid_email,
                "password": "ValidPassword123!",  # pragma: allowlist secret
                "name": "Test User",
                "terms_accepted": True,
            }

            response = await http_client.post(
                "/api/v1/auth/register", json=registration_data
            )

            # Should return 400 Bad Request for validation error
            assert response.status_code == 400
            error = response.json()["error"]
            assert error["code"] == "VALIDATION_ERROR"

    async def test_registration_weak_password_rejected(
        self,
        http_client: AsyncClient,
        visitor_user: UserPersona,
    ):
        """Test that weak passwords are rejected."""
        weak_passwords = [  # pragma: allowlist secret
            "123",  # Too short
            "password",  # Too common  # pragma: allowlist secret
            "PASSWORD",  # No lowercase/numbers
            "12345678",  # Only numbers
        ]

        for weak_password in weak_passwords:
            registration_data = {
                "email": visitor_user.email,
                "password": weak_password,
                "name": visitor_user.name,
                "terms_accepted": True,
            }

            response = await http_client.post(
                "/api/v1/auth/register", json=registration_data
            )

            # Should return 400 Bad Request
            assert response.status_code == 400
            error = response.json()["error"]
            assert error["code"] == "VALIDATION_ERROR"
            assert "password" in error["message"].lower()

    async def test_registration_requires_terms_acceptance(
        self,
        http_client: AsyncClient,
        visitor_user: UserPersona,
    ):
        """Test that terms acceptance is required."""
        registration_data = {
            "email": visitor_user.email,
            "password": "SecurePassword123!",  # pragma: allowlist secret
            "name": visitor_user.name,
            "terms_accepted": False,  # Not accepted
        }

        response = await http_client.post(
            "/api/v1/auth/register", json=registration_data
        )

        # Should return 400 Bad Request
        assert response.status_code == 400
        error = response.json()["error"]
        assert error["code"] == "VALIDATION_ERROR"
        assert "terms" in error["message"].lower()


@pytest.mark.auth
@pytest.mark.real_services
class TestEmailVerification:
    """US-002: Email verification link handling and expiration."""

    async def test_email_verification_success(
        self,
        http_client: AsyncClient,
    ):
        """Test successful email verification."""
        # Mock verification token
        verification_token = "valid_verification_token_123"  # noqa: S105

        response = await http_client.post(
            "/api/v1/auth/verify-email", json={"token": verification_token}
        )

        # Should return 200 OK
        assert response.status_code == 200
        data = response.json()

        assert "message" in data
        assert "verified" in data["message"].lower()

    async def test_email_verification_invalid_token(
        self,
        http_client: AsyncClient,
    ):
        """Test email verification with invalid token."""
        invalid_token = "invalid_or_expired_token"  # noqa: S105

        response = await http_client.post(
            "/api/v1/auth/verify-email", json={"token": invalid_token}
        )

        # Should return 400 Bad Request
        assert response.status_code == 400
        error = response.json()["error"]
        assert error["code"] == "VALIDATION_ERROR"

    async def test_email_verification_expired_token(
        self,
        http_client: AsyncClient,
    ):
        """Test email verification with expired token."""
        expired_token = "expired_verification_token"  # noqa: S105

        response = await http_client.post(
            "/api/v1/auth/verify-email", json={"token": expired_token}
        )

        # Should return 410 Gone for expired token
        assert response.status_code == 410
        error = response.json()["error"]
        assert "expired" in error["message"].lower()


@pytest.mark.auth
@pytest.mark.real_services
class TestUserLogin:
    """US-003: Login with email/password authentication."""

    async def test_valid_login_returns_jwt(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test successful login with valid credentials."""
        login_data = {
            "email": starter_user.email,
            "password": "CorrectPassword123!",  # pragma: allowlist secret
        }

        response = await http_client.post("/api/v1/auth/login", json=login_data)

        # Should return 200 OK
        assert response.status_code == 200
        data = response.json()

        # Verify JWT token structure
        assert "access_token" in data
        assert "token_type" in data
        assert data["token_type"] == "bearer"  # noqa: S105
        assert "expires_in" in data

        # Verify user information
        assert "user" in data
        user_data = data["user"]
        assert_valid_urn(user_data["id"], "user")
        assert user_data["email"] == starter_user.email
        assert_user_tier_valid(user_data["tier"])

        # JWT should be a properly formatted token
        token = data["access_token"]
        assert isinstance(token, str)
        assert len(token.split(".")) == 3  # Header.Payload.Signature

    async def test_invalid_credentials_rejected(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test that invalid credentials are rejected."""
        invalid_credentials = [
            {
                "email": starter_user.email,
                "password": "WrongPassword123!",  # pragma: allowlist secret
            },
            {
                "email": "nonexistent@example.com",
                "password": "AnyPassword123!",  # pragma: allowlist secret
            },
            {
                "email": starter_user.email,
                "password": "",
            },
        ]

        for credentials in invalid_credentials:
            response = await http_client.post("/api/v1/auth/login", json=credentials)

            # Should return 401 Unauthorized
            assert response.status_code == 401
            error = response.json()["error"]
            assert error["code"] == "AUTHENTICATION_ERROR"

    async def test_unverified_email_login_rejected(
        self,
        http_client: AsyncClient,
    ):
        """Test that users with unverified emails cannot login."""
        login_data = {
            "email": "unverified@example.com",
            "password": "ValidPassword123!",  # pragma: allowlist secret
        }

        response = await http_client.post("/api/v1/auth/login", json=login_data)

        # Should return 403 Forbidden
        assert response.status_code == 403
        error = response.json()["error"]
        assert error["code"] == "AUTHORIZATION_ERROR"
        assert "verification" in error["message"].lower()


@pytest.mark.auth
@pytest.mark.real_services
class TestPasswordReset:
    """US-004: Password reset flow with secure token handling."""

    async def test_password_reset_request(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test password reset request."""
        reset_data = {"email": starter_user.email}

        response = await http_client.post(
            "/api/v1/auth/reset-password", json=reset_data
        )

        # Should return 200 OK even for non-existent emails (security)
        assert response.status_code == 200
        data = response.json()
        assert "message" in data
        assert "reset" in data["message"].lower()

    async def test_password_reset_confirmation(self, http_client: AsyncClient):
        """Test password reset with valid token."""
        reset_data = {
            "token": "valid_reset_token",
            "new_password": "NewSecurePassword456!",  # pragma: allowlist secret
        }

        response = await http_client.post("/api/v1/auth/confirm-reset", json=reset_data)

        # Should return 200 OK
        assert response.status_code == 200
        data = response.json()
        assert "message" in data
        assert "updated" in data["message"].lower()

    async def test_password_reset_invalid_token(self, http_client: AsyncClient):
        """Test password reset with invalid token."""
        reset_data = {
            "token": "invalid_token",
            "new_password": "NewSecurePassword456!",  # pragma: allowlist secret
        }

        response = await http_client.post("/api/v1/auth/confirm-reset", json=reset_data)

        # Should return 400 Bad Request
        assert response.status_code == 400
        error = response.json()["error"]
        assert error["code"] == "VALIDATION_ERROR"


@pytest.mark.auth
@pytest.mark.real_services
class TestSessionManagement:
    """US-009: Session management and JWT token validation."""

    async def test_jwt_token_validation(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test that valid JWT tokens are accepted."""
        # Use valid JWT token for authenticated request
        headers = {"Authorization": f"Bearer {starter_user.to_auth_token()}"}

        response = await http_client.get("/api/v1/profile", headers=headers)

        # Should return 200 OK for authenticated request
        assert response.status_code == 200
        data = response.json()

        assert "user" in data
        user_data = data["user"]
        assert user_data["email"] == starter_user.email
        assert_valid_urn(user_data["id"], "user")

    async def test_invalid_jwt_rejected(self, http_client: AsyncClient):
        """Test that invalid JWT tokens are rejected."""
        invalid_tokens = [
            "invalid.jwt.token",
            "Bearer invalid_token",
            "expired.jwt.token",
            "",
        ]

        for token in invalid_tokens:
            headers = {"Authorization": f"Bearer {token}"}
            response = await http_client.get("/api/v1/profile", headers=headers)

            # Should return 401 Unauthorized
            assert response.status_code == 401
            error = response.json()["error"]
            assert error["code"] == "AUTHENTICATION_ERROR"

    async def test_missing_authorization_header(self, http_client: AsyncClient):
        """Test that requests without authorization are rejected."""
        response = await http_client.get("/api/v1/profile")

        # Should return 401 Unauthorized
        assert response.status_code == 401
        error = response.json()["error"]
        assert error["code"] == "AUTHENTICATION_ERROR"

    async def test_token_logout_invalidation(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test that tokens are invalidated after logout."""
        token = starter_user.to_auth_token()
        headers = {"Authorization": f"Bearer {token}"}

        # First, verify token works
        response = await http_client.get("/api/v1/profile", headers=headers)
        assert response.status_code == 200

        # Logout
        response = await http_client.post("/api/v1/auth/logout", headers=headers)
        assert response.status_code == 200

        # Token should now be invalid
        response = await http_client.get("/api/v1/profile", headers=headers)
        assert response.status_code == 401


@pytest.mark.auth
@pytest.mark.security
@pytest.mark.real_services
class TestRateLimiting:
    """US-007: Rate limiting on authentication endpoints."""

    async def test_login_rate_limiting(
        self,
        http_client: AsyncClient,
        starter_user: UserPersona,
    ):
        """Test that excessive login attempts are rate limited."""
        login_data = {
            "email": starter_user.email,
            "password": "WrongPassword",  # pragma: allowlist secret
        }

        # Make multiple failed login attempts
        for _ in range(5):
            await http_client.post("/api/v1/auth/login", json=login_data)

        # Next attempt should be rate limited
        response = await http_client.post("/api/v1/auth/login", json=login_data)

        # Should return 429 Too Many Requests
        assert response.status_code == 429
        error = response.json()["error"]
        assert error["code"] == "RATE_LIMIT_EXCEEDED"

    async def test_registration_rate_limiting(
        self,
        http_client: AsyncClient,
        visitor_user: UserPersona,
    ):
        """Test that excessive registration attempts are rate limited."""
        for i in range(5):
            registration_data = {
                "email": f"test{i}@example.com",
                "password": "ValidPassword123!",  # pragma: allowlist secret
                "name": "Test User",
                "terms_accepted": True,
            }
            await http_client.post("/api/v1/auth/register", json=registration_data)

        # Next attempt should be rate limited
        final_registration = {
            "email": "final@example.com",
            "password": "ValidPassword123!",  # pragma: allowlist secret
            "name": "Final User",
            "terms_accepted": True,
        }

        response = await http_client.post(
            "/api/v1/auth/register", json=final_registration
        )

        # Should return 429 Too Many Requests
        assert response.status_code == 429
        error = response.json()["error"]
        assert error["code"] == "RATE_LIMIT_EXCEEDED"


@pytest.mark.auth
@pytest.mark.integration
@pytest.mark.real_services
class TestOAuthIntegration:
    """US-005: OAuth login integration (Google, GitHub)."""

    async def test_oauth_google_login_redirect(self, http_client: AsyncClient):
        """Test Google OAuth login redirect."""
        response = await http_client.get("/api/v1/auth/oauth/google")

        # Should return redirect to Google OAuth
        assert response.status_code == 302
        assert "Location" in response.headers
        assert "google" in response.headers["Location"].lower()

    async def test_oauth_github_login_redirect(self, http_client: AsyncClient):
        """Test GitHub OAuth login redirect."""
        response = await http_client.get("/api/v1/auth/oauth/github")

        # Should return redirect to GitHub OAuth
        assert response.status_code == 302
        assert "Location" in response.headers
        assert "github" in response.headers["Location"].lower()

    async def test_oauth_callback_success(self, http_client: AsyncClient):
        """Test successful OAuth callback handling."""
        # Mock OAuth callback with authorization code
        callback_params = {
            "code": "mock_auth_code_123",
            "state": "secure_random_state",
        }

        response = await http_client.get(
            "/api/v1/auth/oauth/google/callback", params=callback_params
        )

        # Should return 200 OK with JWT token
        assert response.status_code == 200
        data = response.json()

        assert "access_token" in data
        assert "user" in data
        user_data = data["user"]
        assert_valid_urn(user_data["id"], "user")
        assert_user_tier_valid(user_data["tier"])

    async def test_oauth_callback_invalid_code(self, http_client: AsyncClient):
        """Test OAuth callback with invalid authorization code."""
        callback_params = {
            "code": "invalid_code",
            "state": "secure_random_state",
        }

        response = await http_client.get(
            "/api/v1/auth/oauth/google/callback", params=callback_params
        )

        # Should return 400 Bad Request
        assert response.status_code == 400
        error = response.json()["error"]
        assert error["code"] == "AUTHENTICATION_ERROR"
