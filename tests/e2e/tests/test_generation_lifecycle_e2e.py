"""Generation Lifecycle E2E Tests.

Tests full user journeys involving generations (15 stories):
  - End-to-end generate flows (GL-01 through GL-05)
  - Multi-domain integration (GL-06 through GL-10)
  - Complex scenarios (GL-11 through GL-15)
"""

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    TestDataFactory,
    complete_generation,
    create_character,
    create_conversation,
    create_ephemeral_generation,
    create_generation_from_artifact,
    fail_generation,
    send_message,
    trigger_callback,
)


@pytest.mark.generations
class TestGenerationLifecycleE2E:
    """Generation lifecycle end-to-end tests."""

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
    # End-to-End Generate Flows (GL-01 through GL-05)
    # -------------------------------------------------------------------

    async def test_gl01_character_create_generate_complete(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-01: Create character -> generate -> started -> completed -> artifact ready."""
        owner = seed_users.owner

        # Create character
        character = await create_character(http_client, owner.auth_headers())

        # Generate from it
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]
        artifact_id = result["artifact"]["id"]

        # Complete the generation
        await complete_generation(
            http_client,
            generation_id,
            output={"url": "https://cdn.example.com/render.png"},
        )

        # Verify generation completed
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

        # Verify artifact is ready
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "ready"

    async def test_gl02_character_create_generate_fail(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-02: Create character -> generate -> started -> failed -> artifact failed."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]
        artifact_id = result["artifact"]["id"]

        # Fail the generation
        await fail_generation(
            http_client,
            generation_id,
            error={"message": "GPU timeout"},
            failure_type="timeout",
        )

        # Verify generation failed
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        gen = resp.json()
        assert gen["status"] == "failed"
        assert gen["failure_type"] == "timeout"

        # Verify artifact failed
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "failed"

    async def test_gl03_generate_cancel_then_clone(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-03: Generate -> cancel -> clone -> new generation queued."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        # Cancel the generation
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Clone the canceled generation
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        cloned = resp.json()
        assert cloned["id"] != generation_id
        assert cloned["status"] == "queued"

    async def test_gl04_ephemeral_generation_full_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-04: Ephemeral generation: create -> start -> progress -> complete -> delete."""
        owner = seed_users.owner

        # Create
        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        # Start
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Progress
        resp = await trigger_callback(
            http_client, generation_id, "progress", progress_percent=50.0
        )
        assert resp.status_code == 200

        # Complete
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/result.mp4"},
            output_size_bytes=99999,
        )
        assert resp.status_code == 200

        # Verify completed state
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

        # Delete
        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Verify gone
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_gl05_generate_with_progress_updates(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-05: Generate with multiple progress updates before completion."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]

        # Start
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Multiple progress updates
        for pct in [10.0, 25.0, 50.0, 75.0, 90.0]:
            resp = await trigger_callback(
                http_client, generation_id, "progress", progress_percent=pct
            )
            assert resp.status_code == 200

        # Complete
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/final.png"},
            output_size_bytes=54321,
        )
        assert resp.status_code == 200

        # Check final state
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

    # -------------------------------------------------------------------
    # Multi-Domain Integration (GL-06 through GL-10)
    # -------------------------------------------------------------------

    async def test_gl06_conversation_character_generate_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-06: Conversation -> generate character -> generate -> complete."""
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

        # Generate from the character
        gen_result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character_id
        )
        generation_id = gen_result["generation"]["id"]

        # Complete the generation
        await complete_generation(http_client, generation_id)

        # Verify all resources
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

        resp = await http_client.get("/v1/artifacts", headers=owner.auth_headers())
        assert resp.status_code == 200
        artifact_kinds = {a["kind"] for a in resp.json()}
        assert "character" in artifact_kinds
        assert "image" in artifact_kinds

    async def test_gl07_api_key_generate_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-07: API key -> create character -> generate -> complete."""
        owner = seed_users.owner

        _, raw_key = await self._create_api_key(http_client, owner.auth_headers())
        api_headers = {"Authorization": f"Bearer {raw_key}"}

        # Create character via API key
        character = await create_character(http_client, api_headers)

        # Generate via API key
        result = await create_generation_from_artifact(
            http_client, api_headers, character["id"]
        )
        generation_id = result["generation"]["id"]

        # Complete
        await complete_generation(http_client, generation_id)

        # Verify via API key
        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=api_headers
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "completed"

    async def test_gl08_team_scoped_generate_lifecycle(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
        test_data_factory: TestDataFactory,
    ):
        """GL-08: Team-scoped character -> generate -> generation owned by team."""
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

        # Generate from the character
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        # Generation should be owned by team
        assert result["generation"]["owner"] == team_urn

    async def test_gl09_multiple_generates_from_same_character(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-09: Multiple generates from same character create independent generations."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())

        result1 = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        result2 = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        # Different generations and artifacts
        assert result1["generation"]["id"] != result2["generation"]["id"]
        assert result1["artifact"]["id"] != result2["artifact"]["id"]

        # Complete first, fail second
        await complete_generation(http_client, result1["generation"]["id"])
        await fail_generation(http_client, result2["generation"]["id"])

        # Verify independent states
        resp = await http_client.get(
            f"/v1/generations/{result1['generation']['id']}",
            headers=owner.auth_headers(),
        )
        assert resp.json()["status"] == "completed"

        resp = await http_client.get(
            f"/v1/generations/{result2['generation']['id']}",
            headers=owner.auth_headers(),
        )
        assert resp.json()["status"] == "failed"

    async def test_gl10_generation_list_mixed_with_generate_and_ephemeral(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-10: Generation list shows both artifact-based and ephemeral generations."""
        owner = seed_users.owner

        # Create ephemeral generation
        ephemeral = await create_ephemeral_generation(http_client, owner.auth_headers())

        # Create generation from artifact
        character = await create_character(http_client, owner.auth_headers())
        gen_result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )

        # List should contain both
        resp = await http_client.get("/v1/generations", headers=owner.auth_headers())
        assert resp.status_code == 200
        generation_ids = {g["id"] for g in resp.json()}
        assert ephemeral["id"] in generation_ids
        assert gen_result["generation"]["id"] in generation_ids

    # -------------------------------------------------------------------
    # Complex Scenarios (GL-11 through GL-15)
    # -------------------------------------------------------------------

    async def test_gl11_clone_and_complete_cloned_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-11: Complete generation -> clone -> complete clone -> both completed."""
        owner = seed_users.owner

        # Create and complete original
        original = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "Original"}
        )
        await complete_generation(http_client, original["id"])

        # Clone
        resp = await http_client.post(
            f"/v1/generations/{original['id']}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        clone = resp.json()

        # Complete the clone
        await complete_generation(http_client, clone["id"])

        # Both completed
        resp = await http_client.get(
            f"/v1/generations/{original['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

        resp = await http_client.get(
            f"/v1/generations/{clone['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

    async def test_gl12_fail_then_clone_then_succeed(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-12: Generation fails -> clone -> new generation succeeds (retry pattern)."""
        owner = seed_users.owner

        # Create and fail
        original = await create_ephemeral_generation(
            http_client,
            owner.auth_headers(),
            spec={"prompt": "Retry this"},
        )
        await fail_generation(http_client, original["id"])

        # Clone (retry)
        resp = await http_client.post(
            f"/v1/generations/{original['id']}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        retry = resp.json()

        # Complete the retry
        await complete_generation(http_client, retry["id"])

        # Original still failed, retry completed
        resp = await http_client.get(
            f"/v1/generations/{original['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "failed"

        resp = await http_client.get(
            f"/v1/generations/{retry['id']}", headers=owner.auth_headers()
        )
        assert resp.json()["status"] == "completed"

    async def test_gl13_delete_completed_generation_artifact_persists(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-13: Delete completed generation -> output artifact persists."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]
        artifact_id = result["artifact"]["id"]

        await complete_generation(http_client, generation_id)

        # Delete the generation
        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204, (
            f"DELETE generation failed: {resp.status_code} {resp.text}"
        )

        # Artifact should still exist
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "ready"

    async def test_gl14_cancel_generation_artifact_stays_pending(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-14: Cancel generation -> output artifact stays pending or becomes failed."""
        owner = seed_users.owner

        character = await create_character(http_client, owner.auth_headers())
        result = await create_generation_from_artifact(
            http_client, owner.auth_headers(), character["id"]
        )
        generation_id = result["generation"]["id"]
        artifact_id = result["artifact"]["id"]

        # Cancel the generation
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Artifact should be pending or failed (implementation-dependent)
        resp = await http_client.get(
            f"/v1/artifacts/{artifact_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["status"] in ["pending", "failed"]

    async def test_gl15_generation_timestamps_progression(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GL-15: Generation timestamps progress correctly through lifecycle."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        created_at = gen["created_at"]

        # Start
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        started_gen = resp.json()
        assert started_gen.get("started_at") is not None
        assert started_gen["started_at"] >= created_at

        # Complete
        resp = await trigger_callback(
            http_client,
            generation_id,
            "completed",
            output={"url": "https://example.com/done.png"},
            output_size_bytes=100,
        )
        assert resp.status_code == 200

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        completed_gen = resp.json()
        assert completed_gen["completed_at"] is not None
        assert completed_gen["completed_at"] >= completed_gen["started_at"]
        assert completed_gen["created_at"] <= completed_gen["started_at"]
