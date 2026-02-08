---
name: python-e2e
description: Python E2E testing patterns. Use when writing, debugging, or understanding E2E tests in tests/e2e/.
---

# Python E2E Tests

## Philosophy

E2E tests verify the **entire system** from an external client perspective. Written in Python because:

- Industry standard for test automation
- Rich ecosystem (pytest, httpx)
- Faster iteration than compiled tests

## Project Configuration

```toml
# tests/e2e/pyproject.toml
[project]
name = "framecast-e2e-tests"
dependencies = [
    "pytest>=8.0.0",
    "pytest-asyncio>=0.23.0",
    "httpx>=0.26.0",
    "faker>=21.0.0",
    "pydantic>=2.5.0",
    "pydantic-settings>=2.1.0",
    "PyJWT>=2.8.0",
    "asyncpg>=0.29.0",
]

[tool.pytest.ini_options]
asyncio_mode = "auto"
testpaths = ["tests"]
markers = [
    "slow: marks tests as slow",
    "auth: authentication related tests",
    "teams: team management tests",
    "security: security validation tests",
    "invitation: invitation workflow tests",
    "error_handling: error handling and edge case tests",
]
```

## Directory Structure

```
tests/e2e/
├── pyproject.toml
├── uv.lock
├── conftest.py              # Shared fixtures, UserPersona, E2EConfig
├── utils/
│   └── localstack_email.py  # LocalStack SES email retrieval client
└── tests/
    ├── test_*.py             # E2E test files (user stories)
    └── ...
```

## Key Components

### UserPersona

Test users with JWT auth built-in:

```python
class UserPersona(BaseModel):
    user_id: str
    email: str
    name: str
    tier: str  # "starter", "creator"
    credits: int = 0

    def to_auth_token(self) -> str:
        """Generate HS256 JWT token."""
        ...

    def auth_headers(self) -> dict[str, str]:
        """Return Authorization header dict."""
        ...
```

### E2EConfig

Loaded from environment variables with `TEST_` prefix:

```python
class E2EConfig(BaseSettings):
    local_api_url: str = "http://localhost:3000"
    database_url: str = "..."
    localstack_ses_url: str = "http://localhost:4566"
    model_config = ConfigDict(env_prefix="TEST_", env_file=".env.test")
```

### Core Fixtures

- `test_config` — session-scoped E2EConfig
- `http_client` — httpx.AsyncClient pointed at API
- `seed_users` — seeds owner + invitee into DB, yields SeededUsers, truncates after
- `localstack_email_client` — LocalStack SES client for email verification
- `test_data_factory` — TestDataFactory for generating team data

### LocalStack Email Client

For verifying invitation emails sent via SES:

```python
client = LocalStackEmailClient("http://localhost:4566")
emails = await client.get_emails_for("invitee@example.com")
```

Note: LocalStack `/_aws/ses` filters by **sender** only. The client fetches all emails and filters by recipient client-side.

## Just Targets

```bash
just test-e2e           # Run all E2E tests
just ci-test-e2e        # Run in CI mode
```
