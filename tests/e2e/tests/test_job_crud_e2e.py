"""Job CRUD E2E Tests.

Tests job lifecycle operations (25 stories):
  - Create ephemeral jobs (J-01 through J-06)
  - Read jobs (J-07 through J-12)
  - Cancel jobs (J-13 through J-17)
  - Delete jobs (J-18 through J-23)
  - Clone jobs (J-24 through J-25)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    complete_job,
    create_ephemeral_job,
    fail_job,
    trigger_callback,
)


@pytest.mark.jobs
class TestJobCrudE2E:
    """Job CRUD end-to-end tests."""

    # -------------------------------------------------------------------
    # Create Ephemeral Jobs (J-01 through J-06)
    # -------------------------------------------------------------------

    async def test_j01_empty_job_list_for_new_user(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-01: GET /v1/jobs empty list for new starter user."""
        invitee = seed_users.invitee

        resp = await http_client.get("/v1/jobs", headers=invitee.auth_headers())
        assert resp.status_code == 200
        jobs = resp.json()
        assert isinstance(jobs, list)
        assert len(jobs) == 0

    async def test_j02_create_ephemeral_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-02: POST /v1/generate creates ephemeral job, status=queued, returns 201."""
        owner = seed_users.owner

        resp = await http_client.post(
            "/v1/generate",
            json={"spec": {"prompt": "A brave warrior"}},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 201
        job = resp.json()
        assert job["status"] == "queued"

    async def test_j03_create_ephemeral_job_response_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-03: POST /v1/generate response has id, owner, status, spec_snapshot, options, created_at."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        assert "id" in job
        assert "owner" in job
        assert "status" in job
        assert "spec_snapshot" in job
        assert "created_at" in job

    async def test_j04_create_ephemeral_job_owner_default(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-04: POST /v1/generate owner defaults to framecast:user:{user_id}."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        expected_urn = f"framecast:user:{owner.user_id}"
        assert job["owner"] == expected_urn

    async def test_j05_create_ephemeral_job_spec_preserved(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-05: POST /v1/generate spec_snapshot preserved from input."""
        owner = seed_users.owner

        spec = {"prompt": "A dragon breathing fire", "style": "anime"}
        job = await create_ephemeral_job(http_client, owner.auth_headers(), spec=spec)
        assert job["spec_snapshot"]["prompt"] == "A dragon breathing fire"
        assert job["spec_snapshot"]["style"] == "anime"

    async def test_j06_create_ephemeral_job_options_stored(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-06: POST /v1/generate options stored when provided."""
        owner = seed_users.owner

        options = {"resolution": "1920x1080", "quality": "high"}
        job = await create_ephemeral_job(
            http_client, owner.auth_headers(), options=options
        )
        assert job.get("options") is not None
        assert job["options"]["resolution"] == "1920x1080"
        assert job["options"]["quality"] == "high"

    # -------------------------------------------------------------------
    # Read Jobs (J-07 through J-12)
    # -------------------------------------------------------------------

    async def test_j07_get_job_by_id(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-07: GET /v1/jobs/:id returns job with all fields."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 200
        fetched = resp.json()
        assert fetched["id"] == job_id
        assert fetched["status"] == "queued"
        assert "owner" in fetched
        assert "spec_snapshot" in fetched
        assert "created_at" in fetched

    async def test_j08_get_nonexistent_job_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-08: GET /v1/jobs/:id nonexistent returns 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/jobs/{fake_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 404

    async def test_j09_list_jobs_ordered_by_created_at_desc(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-09: GET /v1/jobs after creating 3 jobs, list returns 3 ordered by created_at DESC."""
        owner = seed_users.owner

        job1 = await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "First"}
        )
        job2 = await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "Second"}
        )
        job3 = await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "Third"}
        )

        resp = await http_client.get("/v1/jobs", headers=owner.auth_headers())
        assert resp.status_code == 200
        jobs = resp.json()
        assert len(jobs) >= 3

        # Verify ordering: most recent first
        job_ids = [j["id"] for j in jobs]
        assert job_ids.index(job3["id"]) < job_ids.index(job2["id"])
        assert job_ids.index(job2["id"]) < job_ids.index(job1["id"])

    async def test_j10_list_jobs_filter_by_status_queued(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-10: GET /v1/jobs filter by status=queued."""
        owner = seed_users.owner

        # Create a job (stays queued)
        await create_ephemeral_job(http_client, owner.auth_headers())

        resp = await http_client.get(
            "/v1/jobs", params={"status": "queued"}, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        jobs = resp.json()
        assert len(jobs) >= 1
        for job in jobs:
            assert job["status"] == "queued"

    async def test_j11_list_jobs_filter_by_status_completed(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-11: GET /v1/jobs filter by status=completed."""
        owner = seed_users.owner

        # Create and complete a job
        job = await create_ephemeral_job(http_client, owner.auth_headers())
        await complete_job(http_client, job["id"])

        resp = await http_client.get(
            "/v1/jobs", params={"status": "completed"}, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        jobs = resp.json()
        assert len(jobs) >= 1
        for j in jobs:
            assert j["status"] == "completed"

    async def test_j12_list_jobs_limit(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-12: GET /v1/jobs limit=2 returns at most 2."""
        owner = seed_users.owner

        # Create 3 jobs
        await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "A"}
        )
        await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "B"}
        )
        await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "C"}
        )

        resp = await http_client.get(
            "/v1/jobs", params={"limit": 2}, headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        jobs = resp.json()
        assert len(jobs) <= 2

    # -------------------------------------------------------------------
    # Cancel Jobs (J-13 through J-17)
    # -------------------------------------------------------------------

    async def test_j13_cancel_queued_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-13: POST /v1/jobs/:id/cancel cancel queued job -> status=canceled, failure_type=canceled."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        canceled = resp.json()
        assert canceled["status"] == "canceled"
        assert canceled["failure_type"] == "canceled"

    async def test_j14_cancel_processing_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-14: POST /v1/jobs/:id/cancel cancel processing job -> status=canceled."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        # Move to processing via started callback
        resp = await trigger_callback(http_client, job_id, "started")
        assert resp.status_code == 200

        # Cancel the processing job
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        canceled = resp.json()
        assert canceled["status"] == "canceled"

    async def test_j15_cancel_completed_job_returns_409(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-15: POST /v1/jobs/:id/cancel cancel completed job -> 409."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]
        await complete_job(http_client, job_id)

        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 409

    async def test_j16_cancel_already_canceled_job_returns_409(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-16: POST /v1/jobs/:id/cancel cancel already canceled job -> 409."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        # Cancel once
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        # Cancel again -> 409
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 409

    async def test_j17_cancel_sets_completed_at(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-17: POST /v1/jobs/:id/cancel completed_at set after cancel."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        canceled = resp.json()
        assert canceled["completed_at"] is not None

    # -------------------------------------------------------------------
    # Delete Jobs (J-18 through J-23)
    # -------------------------------------------------------------------

    async def test_j18_delete_completed_ephemeral_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-18: DELETE /v1/jobs/:id delete completed ephemeral job -> 204."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]
        await complete_job(http_client, job_id)

        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_j19_delete_failed_ephemeral_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-19: DELETE /v1/jobs/:id delete failed ephemeral job -> 204."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]
        await fail_job(http_client, job_id)

        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_j20_delete_canceled_ephemeral_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-20: DELETE /v1/jobs/:id delete canceled ephemeral job -> 204."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        # Cancel it first
        resp = await http_client.post(
            f"/v1/jobs/{job_id}/cancel", headers=owner.auth_headers()
        )
        assert resp.status_code == 200

        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

    async def test_j21_delete_queued_job_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-21: DELETE /v1/jobs/:id delete queued job -> 400."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 400

    async def test_j22_delete_processing_job_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-22: DELETE /v1/jobs/:id delete processing job -> 400."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        # Move to processing
        resp = await trigger_callback(http_client, job_id, "started")
        assert resp.status_code == 200

        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 400

    async def test_j23_delete_job_then_get_returns_404(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-23: DELETE /v1/jobs/:id after delete, GET returns 404."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]
        await complete_job(http_client, job_id)

        resp = await http_client.delete(
            f"/v1/jobs/{job_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        resp = await http_client.get(f"/v1/jobs/{job_id}", headers=owner.auth_headers())
        assert resp.status_code == 404

    # -------------------------------------------------------------------
    # Clone Jobs (J-24 through J-25)
    # -------------------------------------------------------------------

    async def test_j24_clone_completed_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-24: POST /v1/jobs/:id/clone clone completed job -> 201, new ID, same spec."""
        owner = seed_users.owner

        spec = {"prompt": "A mighty wizard", "style": "fantasy"}
        job = await create_ephemeral_job(http_client, owner.auth_headers(), spec=spec)
        job_id = job["id"]
        await complete_job(http_client, job_id)

        resp = await http_client.post(
            f"/v1/jobs/{job_id}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 201
        cloned = resp.json()
        assert cloned["id"] != job_id
        assert cloned["status"] == "queued"
        assert cloned["spec_snapshot"]["prompt"] == "A mighty wizard"
        assert cloned["spec_snapshot"]["style"] == "fantasy"

    async def test_j25_clone_queued_job_returns_400(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """J-25: POST /v1/jobs/:id/clone clone queued job -> 400."""
        owner = seed_users.owner

        job = await create_ephemeral_job(http_client, owner.auth_headers())
        job_id = job["id"]

        resp = await http_client.post(
            f"/v1/jobs/{job_id}/clone", headers=owner.auth_headers()
        )
        assert resp.status_code == 400
