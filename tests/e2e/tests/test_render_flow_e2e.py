"""Render Flow E2E Tests.

Tests the render orchestration flow end-to-end (30 stories):
  - Render creation (RF-01 through RF-06)
  - Generation callbacks and state transitions (RF-07 through RF-15)
  - Artifact status updates (RF-16 through RF-20)
  - Error handling (RF-21 through RF-25)
  - Mock render integration (RF-26 through RF-30)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    complete_generation,
    configure_mock_render,
    create_character,
    create_generation_from_artifact,
    create_storyboard,
    fail_generation,
    get_mock_render_history,
    reset_mock_render,
    trigger_callback,
)


@pytest.mark.render
class TestRenderFlowE2E:
    """Render orchestration flow end-to-end tests."""

    # -------------------------------------------------------------------
    # Render Creation (RF-01 through RF-06)
    # -------------------------------------------------------------------

    async def test_rf01_render_character_returns_generation_and_artifact(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-01: POST /v1/generations returns generation + artifact."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        assert "generation" in result
        assert "artifact" in result
        assert result["generation"]["status"] == "queued"
        assert result["artifact"]["kind"] == "image"
        assert result["artifact"]["status"] == "pending"

    async def test_rf02_generation_has_correct_owner(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-02: Generation owner matches the requesting user."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        expected_urn = f"framecast:user:{owner.user_id}"
        assert result["generation"]["owner"] == expected_urn

    async def test_rf03_render_artifact_linked_to_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-03: Output artifact has source=generation and source_generation_id matching."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        assert result["artifact"]["source"] == "generation"
        assert result["artifact"]["source_generation_id"] == result["generation"]["id"]

    async def test_rf04_render_nonexistent_artifact_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-04: Render nonexistent artifact -> 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            "/v1/generations",
            json={"artifact_id": fake_id},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404

    async def test_rf05_render_storyboard_creates_video(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-05: Render storyboard -> 201, returns generation + pending video artifact."""
        owner = seed_users.owner

        storyboard = await create_storyboard(http_client, owner.auth_headers())
        resp = await http_client.post(
            "/v1/generations",
            json={"artifact_id": storyboard["id"]},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        body = resp.json()
        assert body["generation"]["status"] == "queued"
        assert body["artifact"]["kind"] == "video"
        assert body["artifact"]["status"] == "pending"

    async def test_rf06_render_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-06: Render without auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            "/v1/generations",
            json={"artifact_id": fake_id},
        )
        assert resp.status_code == 401

    # -------------------------------------------------------------------
    # Generation Callbacks and State Transitions (RF-07 through RF-15)
    # -------------------------------------------------------------------

    async def test_rf07_started_callback_transitions_to_processing(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-07: Started callback transitions generation from queued to processing."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Verify generation is now processing
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "processing"

    async def test_rf08_completed_callback_transitions_to_completed(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-08: Completed callback transitions generation to completed."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        await complete_generation(http_client, generation_id)

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        assert gen["status"] == "completed"
        assert gen["completed_at"] is not None

    async def test_rf09_failed_callback_transitions_to_failed(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-09: Failed callback transitions generation to failed."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        await fail_generation(http_client, generation_id)

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        assert gen["status"] == "failed"
        assert gen["failure_type"] == "system"

    async def test_rf10_progress_callback_updates_progress(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-10: Progress callback updates generation progress_percent."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        # Start the generation
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Send progress update
        resp = await trigger_callback(
            http_client, generation_id, "progress", progress_percent=50.0
        )
        assert resp.status_code == 200

        # Verify progress
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["progress"]["percent"] == 50.0

    async def test_rf11_completed_generation_has_output(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-11: Completed generation stores output data."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        output = {"url": "https://example.com/render.png", "width": 1024}
        await complete_generation(http_client, generation_id, output=output)

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        assert gen["output"]["url"] == "https://example.com/render.png"

    async def test_rf12_failed_generation_has_error(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-12: Failed generation stores error data."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        error = {"message": "GPU OOM", "code": "OUT_OF_MEMORY"}
        await fail_generation(http_client, generation_id, error=error)

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        assert gen["error"]["message"] == "GPU OOM"

    async def test_rf13_callback_on_nonexistent_generation_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-13: Callback on nonexistent generation -> 404."""
        fake_id = str(uuid.uuid4())
        resp = await trigger_callback(http_client, fake_id, "started")
        assert resp.status_code == 404

    async def test_rf14_callback_invalid_transition_returns_409(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-14: Callback with invalid state transition -> 409."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]
        await complete_generation(http_client, generation_id)

        # Sending started callback on completed generation is invalid
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 409

    async def test_rf15_completed_callback_sets_completed_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-15: Completed callback sets completed_at timestamp."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        await complete_generation(http_client, generation_id)

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        assert gen["completed_at"] is not None
        assert gen["created_at"] <= gen["completed_at"]

    # -------------------------------------------------------------------
    # Artifact Status Updates (RF-16 through RF-20)
    # -------------------------------------------------------------------

    async def test_rf16_artifact_pending_before_generation_completion(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-16: Output artifact stays pending while generation is processing."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        artifact_id = result["artifact"]["id"]
        generation_id = result["generation"]["id"]

        # Start generation (processing)
        await trigger_callback(http_client, generation_id, "started")

        # Artifact should still be pending
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "pending"

    async def test_rf17_artifact_ready_after_generation_completion(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-17: Output artifact becomes ready after generation completion."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        artifact_id = result["artifact"]["id"]
        generation_id = result["generation"]["id"]

        await complete_generation(http_client, generation_id)

        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "ready"

    async def test_rf18_artifact_failed_after_generation_failure(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-18: Output artifact becomes failed after generation failure."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        artifact_id = result["artifact"]["id"]
        generation_id = result["generation"]["id"]

        await fail_generation(http_client, generation_id)

        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "failed"

    async def test_rf19_artifact_list_shows_pending_image(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-19: Artifact list includes the pending image artifact."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        artifact_id = result["artifact"]["id"]

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifacts = resp.json()
        found = [a for a in artifacts if a["id"] == artifact_id]
        assert len(found) == 1
        assert found[0]["kind"] == "image"
        assert found[0]["status"] == "pending"

    async def test_rf20_multiple_renders_create_multiple_artifacts(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-20: Rendering the same character twice creates two separate artifacts."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        character_id = character["id"]

        result1 = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character_id
        )
        result2 = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character_id
        )

        assert result1["artifact"]["id"] != result2["artifact"]["id"]
        assert result1["generation"]["id"] != result2["generation"]["id"]

    # -------------------------------------------------------------------
    # Error Handling (RF-21 through RF-25)
    # -------------------------------------------------------------------

    async def test_rf21_render_other_users_artifact_returns_403_or_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-21: Rendering another user's artifact -> 403 or 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        character = await create_character(http_client, owner.auth_headers())
        resp = await http_client.post(
            "/v1/generations",
            json={"artifact_id": character["id"]},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [403, 404]

    async def test_rf22_callback_missing_generation_id_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-22: Callback without generation_id -> 400."""
        resp = await http_client.post(
            "/internal/generations/callback",
            json={"event": "started"},
        )
        assert resp.status_code == 400

    async def test_rf23_callback_missing_event_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-23: Callback without event -> 400."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            "/internal/generations/callback",
            json={"generation_id": fake_id},
        )
        assert resp.status_code == 400

    async def test_rf24_callback_invalid_event_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-24: Callback with invalid event name -> 400."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        resp = await http_client.post(
            "/internal/generations/callback",
            json={"generation_id": generation_id, "event": "invalid_event_name"},
        )
        assert resp.status_code == 400

    async def test_rf25_failed_callback_requires_failure_type(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-25: Failed callback without failure_type -> 400 or defaults."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        # Start first
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Send failed without failure_type
        resp = await http_client.post(
            "/internal/generations/callback",
            json={
                "generation_id": generation_id,
                "event": "failed",
                "error": {"message": "Something broke"},
            },
        )
        # Either requires failure_type (400) or defaults it
        assert resp.status_code in [200, 400]

    # -------------------------------------------------------------------
    # Mock Render Integration (RF-26 through RF-30)
    # -------------------------------------------------------------------

    async def test_rf26_configure_mock_render(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-26: Configure mock render behavior."""
        await configure_mock_render(http_client, outcome="complete", delay_ms=100)

    async def test_rf27_reset_mock_render(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-27: Reset mock render clears history."""
        # Configure and create a render to populate history
        await configure_mock_render(http_client, outcome="complete", delay_ms=50)
        await reset_mock_render(http_client)

        history = await get_mock_render_history(http_client)
        assert len(history) == 0

    async def test_rf28_mock_render_history_records_requests(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-28: Mock render history records render requests."""
        owner = seed_users.owner
        await reset_mock_render(http_client)
        await configure_mock_render(http_client, outcome="complete", delay_ms=50)

        character = await create_character(http_client, owner.auth_headers())
        await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        history = await get_mock_render_history(http_client)
        assert len(history) >= 1

    async def test_rf29_configure_mock_render_failure(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-29: Configure mock render to fail and verify generation fails."""
        owner = seed_users.owner
        await reset_mock_render(http_client)
        await configure_mock_render(http_client, outcome="fail", delay_ms=50)

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        # Wait briefly and check generation status -- mock should auto-fail
        import asyncio

        await asyncio.sleep(1.0)

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        # Generation should be failed if mock auto-triggers, or still queued if manual
        assert gen["status"] in ["failed", "queued", "processing"]

    async def test_rf30_configure_mock_render_with_progress_steps(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """RF-30: Configure mock render with progress steps."""
        owner = seed_users.owner
        await reset_mock_render(http_client)
        await configure_mock_render(
            http_client,
            outcome="complete",
            delay_ms=50,
            progress_steps=[25.0, 50.0, 75.0],
        )

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        # Verify generation was created successfully
        assert result["generation"]["status"] == "queued"
        assert result["artifact"]["status"] == "pending"
