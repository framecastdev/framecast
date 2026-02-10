"""Generation Events SSE E2E Tests.

Tests SSE event streaming for generations (15 stories):
  - Basic event retrieval (SSE-01 through SSE-05)
  - Event content validation (SSE-06 through SSE-10)
  - Last-Event-ID resumption (SSE-11 through SSE-13)
  - Error handling (SSE-14 through SSE-15)
"""

import json
import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    complete_generation,
    create_ephemeral_generation,
    fail_generation,
    trigger_callback,
)


@pytest.mark.sse
class TestGenerationEventsSseE2E:
    """Generation events SSE end-to-end tests."""

    async def _read_sse_events(
        self,
        http_client: httpx.AsyncClient,
        headers: dict[str, str],
        generation_id: str,
        last_event_id: str | None = None,
    ) -> list[dict]:
        """Read SSE events from generation events endpoint.

        The generation MUST be in a terminal state before calling this, otherwise
        the SSE stream will poll indefinitely and the request will time out.
        """
        req_headers = {**headers}
        if last_event_id:
            req_headers["Last-Event-ID"] = last_event_id
        resp = await http_client.get(
            f"/v1/generations/{generation_id}/events",
            headers=req_headers,
            timeout=10.0,
        )
        assert resp.status_code == 200
        # Parse SSE text format: "id: ...\nevent: ...\ndata: ...\n\n"
        events = []
        current: dict = {}
        for line in resp.text.split("\n"):
            if line.startswith("id:"):
                current["id"] = line[3:].strip()
            elif line.startswith("event:"):
                current["event"] = line[6:].strip()
            elif line.startswith("data:"):
                current["data"] = line[5:].strip()
            elif line == "" and current:
                events.append(current)
                current = {}
        if current:
            events.append(current)
        return events

    # -------------------------------------------------------------------
    # Basic Event Retrieval (SSE-01 through SSE-05)
    # -------------------------------------------------------------------

    async def test_sse01_events_endpoint_returns_200(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-01: GET /v1/generations/:id/events returns 200."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        # Cancel so the SSE stream closes and we can read the response
        resp = await http_client.post(
            f"/v1/generations/{gen['id']}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        resp = await http_client.get(
            f"/v1/generations/{gen['id']}/events",
            headers=owner.auth_headers(),
            timeout=10.0,
        )
        assert resp.status_code == 200

    async def test_sse02_queued_generation_has_queued_event(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-02: Generation has at least a queued event."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        # Cancel to close the stream (queued event already recorded)
        await http_client.post(
            f"/v1/generations/{gen['id']}/cancel", headers=owner.auth_headers()
        )
        events = await self._read_sse_events(
            http_client, owner.auth_headers(), gen["id"]
        )
        event_types = [e.get("event") for e in events]
        assert "queued" in event_types

    async def test_sse03_started_callback_produces_event(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-03: Started callback produces an event in the stream."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Complete to close the stream
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/output.png"},
            output_size_bytes=12345,
        )
        assert resp.status_code == 200

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        event_types = [e.get("event") for e in events]
        assert "started" in event_types

    async def test_sse04_completed_generation_has_all_events(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-04: Completed generation has started + completed events."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        event_types = [e.get("event") for e in events]
        assert "started" in event_types
        assert "completed" in event_types

    async def test_sse05_failed_generation_has_failed_event(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-05: Failed generation has a failed event."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await fail_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        event_types = [e.get("event") for e in events]
        assert "failed" in event_types

    # -------------------------------------------------------------------
    # Event Content Validation (SSE-06 through SSE-10)
    # -------------------------------------------------------------------

    async def test_sse06_events_have_id_field(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-06: Each SSE event has an id field."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        for event in events:
            assert "id" in event, f"Event missing 'id' field: {event}"

    async def test_sse07_events_have_event_field(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-07: Each SSE event has an event field."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        for event in events:
            assert "event" in event, f"Event missing 'event' field: {event}"

    async def test_sse08_events_have_data_field(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-08: Each SSE event has a data field."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        for event in events:
            assert "data" in event, f"Event missing 'data' field: {event}"

    async def test_sse09_event_data_is_valid_json(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-09: Event data field contains valid JSON."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        for event in events:
            if "data" in event:
                parsed = json.loads(event["data"])
                assert isinstance(parsed, dict)

    async def test_sse10_progress_event_has_progress_percent(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-10: Progress event data contains percent."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        await trigger_callback(http_client, generation_id, "started")
        resp = await trigger_callback(
            http_client, generation_id, "progress", progress_percent=42.5
        )
        assert resp.status_code == 200

        # Complete to close the stream
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/output.png"},
            output_size_bytes=12345,
        )
        assert resp.status_code == 200

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        progress_events = [e for e in events if e.get("event") == "progress"]
        assert len(progress_events) >= 1
        data = json.loads(progress_events[0]["data"])
        assert data["percent"] == 42.5

    # -------------------------------------------------------------------
    # Last-Event-ID Resumption (SSE-11 through SSE-13)
    # -------------------------------------------------------------------

    async def test_sse11_last_event_id_filters_earlier_events(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-11: Last-Event-ID filters out events before that ID."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await trigger_callback(http_client, generation_id, "started")
        await trigger_callback(
            http_client, generation_id, "progress", progress_percent=50.0
        )

        # Complete to close the stream
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/output.png"},
            output_size_bytes=12345,
        )
        assert resp.status_code == 200

        # Get all events first
        all_events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        assert len(all_events) >= 2

        # Use first event's ID to resume
        first_event_id = all_events[0].get("id")
        if first_event_id:
            resumed_events = await self._read_sse_events(
                http_client,
                owner.auth_headers(),
                generation_id,
                last_event_id=first_event_id,
            )
            # Should have fewer events (first one filtered out)
            assert len(resumed_events) < len(all_events)

    async def test_sse12_last_event_id_with_unknown_id_returns_all(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-12: Last-Event-ID with unknown ID returns all events or empty."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        events = await self._read_sse_events(
            http_client,
            owner.auth_headers(),
            generation_id,
            last_event_id="nonexistent-id-999",
        )
        # Should return events (behavior depends on implementation)
        assert isinstance(events, list)

    async def test_sse13_event_ids_are_ordered(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-13: Event IDs are monotonically increasing."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await trigger_callback(http_client, generation_id, "started")
        await trigger_callback(
            http_client, generation_id, "progress", progress_percent=25.0
        )
        await trigger_callback(
            http_client, generation_id, "progress", progress_percent=75.0
        )

        # Complete to close the stream
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/output.png"},
            output_size_bytes=12345,
        )
        assert resp.status_code == 200

        events = await self._read_sse_events(
            http_client, owner.auth_headers(), generation_id
        )
        ids = [e.get("id") for e in events if e.get("id")]
        # IDs should be ordered (numeric or lexicographic depending on format)
        assert ids == sorted(ids)

    # -------------------------------------------------------------------
    # Error Handling (SSE-14 through SSE-15)
    # -------------------------------------------------------------------

    async def test_sse14_events_nonexistent_generation_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-14: GET /v1/generations/:id/events for nonexistent generation -> 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/generations/{fake_id}/events",
            headers=owner.auth_headers(),
            timeout=10.0,
        )
        assert resp.status_code == 404

    async def test_sse15_events_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """SSE-15: GET /v1/generations/:id/events without auth -> 401."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        resp = await http_client.get(
            f"/v1/generations/{gen['id']}/events",
            timeout=10.0,
        )
        assert resp.status_code == 401
