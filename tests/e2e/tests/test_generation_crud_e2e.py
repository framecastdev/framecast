"""Generation CRUD E2E Tests.

Tests generation lifecycle operations (25 stories):
  - Create ephemeral generations (G-01 through G-06)
  - Read generations (G-07 through G-12)
  - Cancel generations (G-13 through G-17)
  - Delete generations (G-18 through G-23)
  - Clone generations (G-24 through G-25)
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
    create_ephemeral_generation,
    fail_generation,
    trigger_callback,
)


@pytest.mark.generations
class TestGenerationCrudE2E:
    """Generation CRUD end-to-end tests."""

    # -------------------------------------------------------------------
    # Create Ephemeral Generations (G-01 through G-06)
    # -------------------------------------------------------------------

    async def test_g01_empty_generation_list_for_new_user(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-01: GET /v1/generations empty list for new starter user."""
        invitee = seed_users.invitee

        resp = await http_client.get("/v1/generations", headers=invitee.auth_headers())
        assert resp.status_code == 200
        generations = resp.json()
        assert isinstance(generations, list)
        assert len(generations) == 0

    async def test_g02_create_ephemeral_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-02: POST /v1/generations creates ephemeral generation, status=queued, returns 201."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/generations",
            json={"spec": {"prompt": "A brave warrior"}},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        gen = resp.json()
        assert gen["status"] == "queued"

    async def test_g03_create_ephemeral_generation_response_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-03: POST /v1/generations response has id, owner, status, spec_snapshot, options, created_at."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        assert "id" in gen
        assert "owner" in gen
        assert "status" in gen
        assert "spec_snapshot" in gen
        assert "created_at" in gen

    async def test_g04_create_ephemeral_generation_owner_default(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-04: POST /v1/generations owner defaults to framecast:user:{user_id}."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        expected_urn = f"framecast:user:{owner.user_id}"
        assert gen["owner"] == expected_urn

    async def test_g05_create_ephemeral_generation_spec_preserved(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-05: POST /v1/generations spec_snapshot preserved from input."""
        owner = seed_users.owner

        spec = {"prompt": "A dragon breathing fire", "style": "anime"}
        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec=spec
        )
        assert gen["spec_snapshot"]["prompt"] == "A dragon breathing fire"
        assert gen["spec_snapshot"]["style"] == "anime"

    async def test_g06_create_ephemeral_generation_options_stored(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-06: POST /v1/generations options stored when provided."""
        owner = seed_users.owner

        options = {"resolution": "1920x1080", "quality": "high"}
        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), options=options
        )
        assert gen.get("options") is not None
        assert gen["options"]["resolution"] == "1920x1080"
        assert gen["options"]["quality"] == "high"

    # -------------------------------------------------------------------
    # Read Generations (G-07 through G-12)
    # -------------------------------------------------------------------

    async def test_g07_get_generation_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-07: GET /v1/generations/:id returns generation with all fields."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        fetched = resp.json()
        assert fetched["id"] == generation_id
        assert fetched["status"] == "queued"
        assert "owner" in fetched
        assert "spec_snapshot" in fetched
        assert "created_at" in fetched

    async def test_g08_get_nonexistent_generation_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-08: GET /v1/generations/:id nonexistent returns 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/generations/{fake_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_g09_list_generations_ordered_by_created_at_desc(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-09: GET /v1/generations after creating 3 generations, list returns 3 ordered by created_at DESC."""
        owner = seed_users.owner

        gen1 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "First"}
        )
        gen2 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "Second"}
        )
        gen3 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "Third"}
        )

        resp = await http_client.get("/v1/generations", headers=owner.auth_headers())
        assert resp.status_code == 200
        generations = resp.json()
        assert len(generations) >= 3

        # Verify ordering: most recent first
        generation_ids = [g["id"] for g in generations]
        assert generation_ids.index(gen3["id"]) < generation_ids.index(gen2["id"])
        assert generation_ids.index(gen2["id"]) < generation_ids.index(gen1["id"])

    async def test_g10_list_generations_filter_by_status_queued(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-10: GET /v1/generations filter by status=queued."""
        owner = seed_users.owner

        # Create a generation (stays queued)
        await create_ephemeral_generation(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/generations", params={"status": "queued"}, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        generations = resp.json()
        assert len(generations) >= 1
        for gen in generations:
            assert gen["status"] == "queued"

    async def test_g11_list_generations_filter_by_status_completed(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-11: GET /v1/generations filter by status=completed."""
        owner = seed_users.owner

        # Create and complete a generation
        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        await complete_generation(http_client, gen["id"])

        resp = await http_client.get(
            "/v1/generations",
            params={"status": "completed"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        generations = resp.json()
        assert len(generations) >= 1
        for g in generations:
            assert g["status"] == "completed"

    async def test_g12_list_generations_limit(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-12: GET /v1/generations limit=2 returns at most 2."""
        owner = seed_users.owner

        # Create 3 generations
        await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "A"}
        )
        await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "B"}
        )
        await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "C"}
        )

        resp = await http_client.get(
            "/v1/generations", params={"limit": 2}, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        generations = resp.json()
        assert len(generations) <= 2

    # -------------------------------------------------------------------
    # Cancel Generations (G-13 through G-17)
    # -------------------------------------------------------------------

    async def test_g13_cancel_queued_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-13: POST /v1/generations/:id/cancel cancel queued generation -> status=canceled, failure_type=canceled."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        canceled = resp.json()
        assert canceled["status"] == "canceled"
        assert canceled["failure_type"] == "canceled"

    async def test_g14_cancel_processing_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-14: POST /v1/generations/:id/cancel cancel processing generation -> status=canceled."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        # Move to processing via started callback
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        # Cancel the processing generation
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        canceled = resp.json()
        assert canceled["status"] == "canceled"

    async def test_g15_cancel_completed_generation_returns_409(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-15: POST /v1/generations/:id/cancel cancel completed generation -> 409."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 409

    async def test_g16_cancel_already_canceled_generation_returns_409(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-16: POST /v1/generations/:id/cancel cancel already canceled generation -> 409."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        # Cancel once
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Cancel again -> 409
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 409

    async def test_g17_cancel_sets_completed_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-17: POST /v1/generations/:id/cancel completed_at set after cancel."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        canceled = resp.json()
        assert canceled["completed_at"] is not None

    # -------------------------------------------------------------------
    # Delete Generations (G-18 through G-23)
    # -------------------------------------------------------------------

    async def test_g18_delete_completed_ephemeral_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-18: DELETE /v1/generations/:id delete completed ephemeral generation -> 204."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_g19_delete_failed_ephemeral_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-19: DELETE /v1/generations/:id delete failed ephemeral generation -> 204."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await fail_generation(http_client, generation_id)

        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_g20_delete_canceled_ephemeral_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-20: DELETE /v1/generations/:id delete canceled ephemeral generation -> 204."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        # Cancel it first
        resp = await http_client.post(
            f"/v1/generations/{generation_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_g21_delete_queued_generation_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-21: DELETE /v1/generations/:id delete queued generation -> 400."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 400

    async def test_g22_delete_processing_generation_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-22: DELETE /v1/generations/:id delete processing generation -> 400."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        # Move to processing
        resp = await trigger_callback(http_client, generation_id, "started")
        assert resp.status_code == 200

        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 400

    async def test_g23_delete_generation_then_get_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-23: DELETE /v1/generations/:id after delete, GET returns 404."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        resp = await http_client.delete(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        resp = await http_client.get(
            f"/v1/generations/{generation_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    # -------------------------------------------------------------------
    # Clone Generations (G-24 through G-25)
    # -------------------------------------------------------------------

    async def test_g24_clone_completed_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-24: POST /v1/generations/:id/clone clone completed generation -> 201, new ID, same spec."""
        owner = seed_users.owner

        spec = {"prompt": "A mighty wizard", "style": "fantasy"}
        gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec=spec
        )
        generation_id = gen["id"]
        await complete_generation(http_client, generation_id)

        resp = await http_client.post(
            f"/v1/generations/{generation_id}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        cloned = resp.json()
        assert cloned["id"] != generation_id
        assert cloned["status"] == "queued"
        assert cloned["spec_snapshot"]["prompt"] == "A mighty wizard"
        assert cloned["spec_snapshot"]["style"] == "fantasy"

    async def test_g25_clone_queued_generation_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """G-25: POST /v1/generations/:id/clone clone queued generation -> 400."""
        owner = seed_users.owner

        gen = await create_ephemeral_generation(http_client, owner.auth_headers())
        generation_id = gen["id"]

        resp = await http_client.post(
            f"/v1/generations/{generation_id}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 400
