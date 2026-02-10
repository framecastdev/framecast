"""Job Lifecycle E2E Tests.

Tests full user journeys involving jobs (15 stories):
  - End-to-end render flows (JL-01 through JL-05)
  - Multi-domain integration (JL-06 through JL-10)
  - Complex scenarios (JL-11 through JL-15)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    TestDataFactory,
    complete_job,
    create_character,
    create_conversation,
    create_ephemeral_job,
    create_render_job,
    fail_job,
    send_message,
    trigger_callback,
)


@pytest.mark.jobs
class TestJobLifecycleE2E:
    """Job lifecycle end-to-end tests."""

    async def _create_api_key(
        self,
        http_client: httpx.AsyncClient,
        headers: dict[str, str],
        name: str = "Lifecycle Test Key",
        scopes: list[str] | None = None,
        owner: str | None = None,
    ) -> tuple[str, str]:
        """Create an API key and return (key_id, raw_key)."""
        payload: dict = {"name": name, "scopes": scopes or ["*"]}
        if owner is not None:
            payload["owner"] = owner
        resp = await http_client.post("/v1/auth/keys", json=payload, headers=headers)
        assert resp.status_code == 201, (
            f"create_api_key failed: {resp.status_code} {resp.text}"
        )
        data = resp.json()
        return data["api_key"]["id"], data["raw_key"]

    # -------------------------------------------------------------------
    # End-to-End Render Flows (JL-01 through JL-05)
    # -------------------------------------------------------------------

    async def test_jl01_character_create_render_complete(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-01: Create character -> render -> started -> completed -> artifact ready."""
        owner = seed_users.owner

        # Create character
        character = await create_character(http_client, owner.auth_headers())

        # Render it
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        job_id = result["job"]["id"]
        artifact_id = result["artifact"]["id"]

        # Complete the job
        await complete_job(
            http_client,
            job_id,
            output={"url": "https://cdn.example.com/render.png"},
        )

        # Verify job completed
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

        # Verify artifact is ready
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "ready"

    async def test_jl02_character_create_render_fail(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-02: Create character -> render -> started -> failed -> artifact failed."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        job_id = result["job"]["id"]
        artifact_id = result["artifact"]["id"]

        # Fail the job
        await fail_job(
            http_client,
            job_id,
            error={"message": "GPU timeout"},
            failure_type="timeout",
        )

        # Verify job failed
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 200
        job = resp.json()
        assert job["status"] == "failed"
        assert job["failure_type"] == "timeout"

        # Verify artifact failed
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "failed"

    async def test_jl03_render_cancel_then_clone(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-03: Render -> cancel -> clone -> new job queued."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        job_id = result["job"]["id"]

        # Cancel the job
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Clone the canceled job
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        cloned = resp.json()
        assert cloned["id"] != job_id
        assert cloned["status"] == "queued"

    async def test_jl04_ephemeral_job_full_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-04: Ephemeral job: create -> start -> progress -> complete -> delete."""
        owner = seed_users.owner

        # Create
        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        # Start
        resp = await trigger_callback(http_client, job_id, "started")
        assert resp.status_code == 200

        # Progress
        resp = await trigger_callback(
            http_client, job_id, "progress", progress_percent=50.0
        )
        assert resp.status_code == 200

        # Complete
        resp = await trigger_callback(
            http_client,
            job_id,
            "completed",
            output={"url": "https://example.com/result.mp4"},
            output_size_bytes=99999,
        )
        assert resp.status_code == 200

        # Verify completed state
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

        # Delete
        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Verify gone
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 404

    async def test_jl05_render_with_progress_updates(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-05: Render with multiple progress updates before completion."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        job_id = result["job"]["id"]

        # Start
        resp = await trigger_callback(http_client, job_id, "started")
        assert resp.status_code == 200

        # Multiple progress updates
        for pct in [10.0, 25.0, 50.0, 75.0, 90.0]:
            resp = await trigger_callback(
                http_client, job_id, "progress", progress_percent=pct
            )
            assert resp.status_code == 200

        # Complete
        resp = await trigger_callback(
            http_client,
            job_id,
            "completed",
            output={"url": "https://example.com/final.png"},
            output_size_bytes=54321,
        )
        assert resp.status_code == 200

        # Check final state
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

    # -------------------------------------------------------------------
    # Multi-Domain Integration (JL-06 through JL-10)
    # -------------------------------------------------------------------

    async def test_jl06_conversation_character_render_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-06: Conversation -> generate character -> render -> complete."""
        owner = seed_users.owner

        # Create conversation and generate character
        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(
            http_client, owner.auth_headers(), conv["id"], "create a character"
        )
        assistant = result["assistant_message"]
        assert assistant["artifacts"] is not None
        assert len(assistant["artifacts"]) > 0
        character_id = assistant["artifacts"][0]["id"]

        # Render the character
        render_result = await create_render_job(
            http_client, owner.auth_headers(), character_id
        )
        job_id = render_result["job"]["id"]

        # Complete the job
        await complete_job(http_client, job_id)

        # Verify all resources
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifact_kinds = {a["kind"] for a in resp.json()}
        assert "character" in artifact_kinds
        assert "image" in artifact_kinds

    async def test_jl07_api_key_render_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-07: API key -> create character -> render -> complete."""
        owner = seed_users.owner

        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # Create character via API key
        character = await create_character(http_client, api_headers)

        # Render via API key
        result = await create_render_job(http_client, api_headers, character["id"])
        job_id = result["job"]["id"]

        # Complete
        await complete_job(http_client, job_id)

        # Verify via API key
        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=api_headers)
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

    async def test_jl08_team_scoped_render_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """JL-08: Team-scoped character -> render -> job owned by team."""
        owner = seed_users.owner

        # Create team
        resp = await http_client.post(
            "/v1/teams",
            json=test_data_factory.team_data(),
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        team_id = resp.json()["id"]
        team_urn = f"framecast:team:{team_id}"

        # Create team-owned character
        character = await create_character(
            http_client, owner.auth_headers(), owner=team_urn
        )
        assert character["owner"] == team_urn

        # Render the character
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )

        # Job should be owned by team
        assert result["job"]["owner"] == team_urn

    async def test_jl09_multiple_renders_from_same_character(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-09: Multiple renders from same character create independent jobs."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())

        result1 = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        result2 = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )

        # Different jobs and artifacts
        assert result1["job"]["id"] != result2["job"]["id"]
        assert result1["artifact"]["id"] != result2["artifact"]["id"]

        # Complete first, fail second
        await complete_job(http_client, result1["job"]["id"])
        await fail_job(http_client, result2["job"]["id"])

        # Verify independent states
        resp = await http_client.get(
            f"/v1/jobs/{result1['job']['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

        resp = await http_client.get(
            f"/v1/jobs/{result2['job']['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "failed"

    async def test_jl10_job_list_mixed_with_render_and_ephemeral(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-10: Job list shows both render and ephemeral jobs."""
        owner = seed_users.owner

        # Create ephemeral job
        ephemeral = await create_ephemeral_job(http_client, owner.auth_headers())

        # Create render job
        character = await create_character(http_client, owner.auth_headers())
        render_result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )

        # List should contain both
        resp = await http_client.get("/v1/jobs", headers=owner.auth_headers())
        assert resp.status_code == 200
        job_ids = {j["id"] for j in resp.json()}
        assert ephemeral["id"] in job_ids
        assert render_result["job"]["id"] in job_ids

    # -------------------------------------------------------------------
    # Complex Scenarios (JL-11 through JL-15)
    # -------------------------------------------------------------------

    async def test_jl11_clone_and_complete_cloned_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-11: Complete job -> clone -> complete clone -> both completed."""
        owner = seed_users.owner

        # Create and complete original
        original = await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "Original"}
        )
        await complete_job(http_client, original["id"])

        # Clone
        resp = await http_client.post(
            f"/v1/jobs/{original['id']}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        clone = resp.json()

        # Complete the clone
        await complete_job(http_client, clone["id"])

        # Both completed
        resp = await http_client.get(
            f"/v1/jobs/{original['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

        resp = await http_client.get(
            f"/v1/jobs/{clone['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

    async def test_jl12_fail_then_clone_then_succeed(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-12: Job fails -> clone -> new job succeeds (retry pattern)."""
        owner = seed_users.owner

        # Create and fail
        original = await create_ephemeral_job(
            http_client,
            owner.auth_headers(),
            spec={"prompt": "Retry this"},
        )
        await fail_job(http_client, original["id"])

        # Clone (retry)
        resp = await http_client.post(
            f"/v1/jobs/{original['id']}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        retry = resp.json()

        # Complete the retry
        await complete_job(http_client, retry["id"])

        # Original still failed, retry completed
        resp = await http_client.get(
            f"/v1/jobs/{original['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "failed"

        resp = await http_client.get(
            f"/v1/jobs/{retry['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

    async def test_jl13_delete_completed_render_job_artifact_persists(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-13: Delete completed render job -> output artifact persists."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        job_id = result["job"]["id"]
        artifact_id = result["artifact"]["id"]

        await complete_job(http_client, job_id)

        # Delete the job
        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Artifact should still exist
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "ready"

    async def test_jl14_cancel_render_job_artifact_stays_pending(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-14: Cancel render job -> output artifact stays pending or becomes failed."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_render_job(
            http_client, owner.auth_headers(), character["id"]
        )
        job_id = result["job"]["id"]
        artifact_id = result["artifact"]["id"]

        # Cancel the job
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Artifact should be pending or failed (implementation-dependent)
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] in ["pending", "failed"]

    async def test_jl15_job_timestamps_progression(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JL-15: Job timestamps progress correctly through lifecycle."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]
        created_at = job["created_at"]

        # Start
        resp = await trigger_callback(http_client, job_id, "started")
        assert resp.status_code == 200

        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        started_job = resp.json()
        assert started_job.get("started_at") is not None
        assert started_job["started_at"] >= created_at

        # Complete
        resp = await trigger_callback(
            http_client,
            job_id,
            "completed",
            output={"url": "https://example.com/done.png"},
            output_size_bytes=100,
        )
        assert resp.status_code == 200

        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        completed_job = resp.json()
        assert completed_job["completed_at"] is not None
        assert completed_job["completed_at"] >= completed_job["started_at"]
        assert completed_job["created_at"] <= completed_job["started_at"]
