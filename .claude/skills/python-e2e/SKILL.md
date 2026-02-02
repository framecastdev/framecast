---
name: python-e2e
description: Python E2E testing patterns. Use when writing, debugging, or understanding E2E tests in tests/e2e/.
---

# Python E2E Tests

## Philosophy

E2E tests verify the **entire system** from an external client perspective. Written in Python because:

- Industry standard for test automation
- Rich ecosystem (pytest, httpx, respx)
- Faster iteration than compiled tests

## Project Configuration

```toml
# tests/e2e/pyproject.toml
[project]
name = "framecast-e2e"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = [
    "pytest>=8.0",
    "pytest-asyncio>=0.23",
    "pytest-timeout>=2.3",
    "pytest-cov>=4.1",
    "httpx>=0.27",
    "respx>=0.21",           # Mock httpx
    "pydantic>=2.6",
    "polyfactory>=2.15",     # Test data factories
    "python-dotenv>=1.0",
]

[project.optional-dependencies]
dev = [
    "ruff>=0.3",
    "mypy>=1.9",
    "pip-audit>=2.7",
]

[tool.pytest.ini_options]
asyncio_mode = "auto"
testpaths = ["tests"]
markers = [
    "real_runpod: requires real RunPod",
    "slow: takes >30 seconds",
]

[tool.ruff]
target-version = "py311"
line-length = 100

[tool.ruff.lint]
select = ["E", "W", "F", "I", "B", "C4", "UP", "ARG", "SIM"]

[tool.mypy]
python_version = "3.11"
strict = true
```

## Directory Structure

```
tests/e2e/
├── pyproject.toml
├── uv.lock
├── conftest.py              # Shared fixtures
├── src/e2e/
│   ├── __init__.py
│   ├── client.py            # Type-safe API client
│   ├── config.py            # Test config
│   ├── fixtures/
│   │   ├── specs.py         # Sample specs
│   │   └── factories.py     # Polyfactory generators
│   ├── mocks/
│   │   ├── anthropic.py     # Mock Anthropic
│   │   └── runpod.py        # Mock RunPod
│   └── utils/
│       ├── polling.py       # Async polling
│       └── assertions.py    # Custom assertions
└── tests/
    ├── test_video_generation.py
    ├── test_job_lifecycle.py
    └── test_credits_refunds.py
```

## Type-Safe API Client

```python
# src/e2e/client.py
from typing import Self
import httpx
from pydantic import BaseModel

class Job(BaseModel):
    id: str
    status: str
    owner: str
    credits_charged: int
    credits_refunded: int = 0

class FramecastClient:
    def __init__(self, base_url: str, token: str | None = None) -> None:
        self.base_url = base_url
        self._client = httpx.AsyncClient(
            base_url=base_url,
            headers={"Authorization": f"Bearer {token}"} if token else {},
            timeout=30.0,
        )

    async def __aenter__(self) -> Self:
        return self

    async def __aexit__(self, *args: object) -> None:
        await self._client.aclose()

    async def create_job(self, spec: dict) -> Job:
        response = await self._client.post("/v1/generate", json={"spec": spec})
        response.raise_for_status()
        return Job.model_validate(response.json())

    async def get_job(self, job_id: str) -> Job:
        response = await self._client.get(f"/v1/jobs/{job_id}")
        response.raise_for_status()
        return Job.model_validate(response.json())
```

## Async Polling Utility

```python
# src/e2e/utils/polling.py
import asyncio
from typing import Callable, TypeVar, Awaitable
from datetime import timedelta

T = TypeVar("T")

class TimeoutError(Exception):
    pass

async def poll_until(
    check: Callable[[], Awaitable[T | None]],
    *,
    timeout: timedelta = timedelta(seconds=60),
    interval: timedelta = timedelta(milliseconds=500),
    description: str = "condition",
) -> T:
    deadline = asyncio.get_event_loop().time() + timeout.total_seconds()

    while asyncio.get_event_loop().time() < deadline:
        result = await check()
        if result is not None:
            return result
        await asyncio.sleep(interval.total_seconds())

    raise TimeoutError(f"Timeout waiting for {description}")
```

## Fixtures

```python
# conftest.py
import pytest
import pytest_asyncio
from e2e.client import FramecastClient
from e2e.config import TestConfig

@pytest.fixture(scope="session")
def config() -> TestConfig:
    return TestConfig.from_env()

@pytest_asyncio.fixture
async def client(config: TestConfig) -> FramecastClient:
    async with FramecastClient(config.api_base_url, config.test_token) as c:
        yield c

@pytest.fixture
def sample_spec() -> dict:
    return {
        "title": "Test Video",
        "scenes": [{"id": "scene1", "prompt": "A cat", "duration": 3}],
    }
```

## Example Test

```python
# tests/test_video_generation.py
import pytest
from datetime import timedelta
from e2e.utils.polling import poll_until

pytestmark = pytest.mark.asyncio

class TestVideoGeneration:
    async def test_generate_mocked(
        self, client, mock_anthropic, mock_runpod, sample_spec
    ) -> None:
        mock_runpod.set_completion_delay(seconds=2)

        job = await client.create_job(sample_spec)
        assert job.status == "queued"

        completed = await poll_until(
            lambda: self._check_terminal(client, job.id),
            timeout=timedelta(seconds=30),
        )
        assert completed.status == "completed"

    @pytest.mark.real_runpod
    @pytest.mark.timeout(300)
    async def test_generate_real(self, client, sample_spec) -> None:
        job = await client.create_job(sample_spec)

        completed = await poll_until(
            lambda: self._check_terminal(client, job.id),
            timeout=timedelta(minutes=5),
        )
        assert completed.status == "completed"

    async def _check_terminal(self, client, job_id):
        job = await client.get_job(job_id)
        return job if job.status in ("completed", "failed", "canceled") else None
```

## Two Test Modes

### Mocked (`just test-e2e-mocked`)

- Uses `respx` to mock Anthropic/RunPod
- Real LocalStack for S3
- Local Inngest
- Fast, deterministic, CI-friendly

### Real RunPod (`just test-e2e-real`)

- Real RunPod execution
- Requires Cloudflare Tunnel (`just tunnel`)
- Slow, costs money
- Mark with `@pytest.mark.real_runpod`

## Just Targets

```bash
just test-e2e-mocked          # Mocked mode
just test-e2e-real            # Real RunPod
just test-e2e-all             # All E2E
just test-e2e "test_name"     # Specific test
just test-e2e-lint            # ruff + mypy
just test-e2e-fmt             # Format
```
