"""Job Concurrency E2E Tests.

Tests concurrency limits and idempotency (15 stories):
  - Starter concurrency limits (JC-01 through JC-04)
  - Creator concurrency limits (JC-05 through JC-08)
  - Idempotency key behavior (JC-09 through JC-13)
  - Edge cases (JC-14 through JC-15)
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
)


@pytest.mark.job_concurrency
class TestJobConcurrencyE2E:
    """Job concurrency end-to-end tests."""

    # -------------------------------------------------------------------
    # Starter Concurrency Limits (JC-01 through JC-04)
    # -------------------------------------------------------------------

    async def test_jc01_starter_can_create_one_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-01: Starter user can create 1 concurrent job."""
        invitee = seed_users.invitee

        job = await create_ephemeral_job(http_client, invitee.auth_headers())
        assert job["status"] == "queued"

    async def test_jc02_starter_second_job_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-02: Starter user's second concurrent job is rejected (CARD-6)."""
        invitee = seed_users.invitee

        # Create first job (stays queued)
        await create_ephemeral_job(http_client, invitee.auth_headers())

        # Second job should be rejected
        resp = await http_client.post(
            "/v1/generate",
            json={"spec": {"prompt": "Second job"}},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409, 429], (
            f"Expected 400/409/429 for starter concurrency limit, got {resp.status_code}"
        )

    async def test_jc03_starter_can_create_after_completion(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-03: Starter can create new job after first one completes."""
        invitee = seed_users.invitee

        # Create and complete first job
        job1 = await create_ephemeral_job(http_client, invitee.auth_headers())
        await complete_job(http_client, job1["id"])

        # Second job should succeed now
        job2 = await create_ephemeral_job(http_client, invitee.auth_headers())
        assert job2["status"] == "queued"

    async def test_jc04_starter_can_create_after_failure(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-04: Starter can create new job after first one fails."""
        invitee = seed_users.invitee

        # Create and fail first job
        job1 = await create_ephemeral_job(http_client, invitee.auth_headers())
        await fail_job(http_client, job1["id"])

        # Second job should succeed now
        job2 = await create_ephemeral_job(http_client, invitee.auth_headers())
        assert job2["status"] == "queued"

    # -------------------------------------------------------------------
    # Creator Concurrency Limits (JC-05 through JC-08)
    # -------------------------------------------------------------------

    async def test_jc05_creator_can_create_multiple_concurrent_jobs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-05: Creator can create multiple concurrent jobs (up to 5)."""
        owner = seed_users.owner

        jobs = []
        for i in range(3):
            job = await create_ephemeral_job(
                http_client, owner.auth_headers(), spec={"prompt": f"Job {i}"}
            )
            jobs.append(job)
            assert job["status"] == "queued"

        assert len(jobs) == 3

    async def test_jc06_creator_can_create_up_to_five_concurrent(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-06: Creator can have up to 5 concurrent jobs (CARD-5)."""
        owner = seed_users.owner

        jobs = []
        for i in range(5):
            job = await create_ephemeral_job(
                http_client, owner.auth_headers(), spec={"prompt": f"Job {i}"}
            )
            jobs.append(job)

        assert len(jobs) == 5

    async def test_jc07_creator_sixth_job_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-07: Creator's 6th concurrent job is rejected (CARD-5)."""
        owner = seed_users.owner

        # Create 5 jobs
        for i in range(5):
            await create_ephemeral_job(
                http_client, owner.auth_headers(), spec={"prompt": f"Job {i}"}
            )

        # 6th should be rejected
        resp = await http_client.post(
            "/v1/generate",
            json={"spec": {"prompt": "Job 6"}},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 409, 429], (
            f"Expected 400/409/429 for creator concurrency limit, got {resp.status_code}"
        )

    async def test_jc08_creator_can_create_after_completing_one(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-08: Creator can create job after completing one when at limit."""
        owner = seed_users.owner

        # Create 5 jobs
        jobs = []
        for i in range(5):
            job = await create_ephemeral_job(
                http_client, owner.auth_headers(), spec={"prompt": f"Job {i}"}
            )
            jobs.append(job)

        # Complete one
        await complete_job(http_client, jobs[0]["id"])

        # Now can create another
        new_job = await create_ephemeral_job(
            http_client, owner.auth_headers(), spec={"prompt": "Replacement"}
        )
        assert new_job["status"] == "queued"

    # -------------------------------------------------------------------
    # Idempotency Key Behavior (JC-09 through JC-13)
    # -------------------------------------------------------------------

    async def test_jc09_idempotency_key_returns_same_job(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-09: Same idempotency key + same user returns existing job."""
        owner = seed_users.owner

        idem_key = str(uuid.uuid4())
        job1 = await create_ephemeral_job(
            http_client, owner.auth_headers(), idempotency_key=idem_key
        )
        job2 = await create_ephemeral_job(
            http_client, owner.auth_headers(), idempotency_key=idem_key
        )

        assert job1["id"] == job2["id"]

    async def test_jc10_different_idempotency_keys_create_different_jobs(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-10: Different idempotency keys create different jobs."""
        owner = seed_users.owner

        key1 = str(uuid.uuid4())
        key2 = str(uuid.uuid4())
        job1 = await create_ephemeral_job(
            http_client, owner.auth_headers(), idempotency_key=key1
        )
        job2 = await create_ephemeral_job(
            http_client, owner.auth_headers(), idempotency_key=key2
        )

        assert job1["id"] != job2["id"]

    async def test_jc11_no_idempotency_key_creates_new_job_each_time(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-11: Without idempotency key, each request creates a new job."""
        owner = seed_users.owner

        job1 = await create_ephemeral_job(http_client, owner.auth_headers())
        job2 = await create_ephemeral_job(http_client, owner.auth_headers())

        assert job1["id"] != job2["id"]

    async def test_jc12_idempotency_key_scoped_to_user(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-12: Same idempotency key from different users creates different jobs."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        idem_key = str(uuid.uuid4())
        job_owner = await create_ephemeral_job(
            http_client, owner.auth_headers(), idempotency_key=idem_key
        )
        job_invitee = await create_ephemeral_job(
            http_client, invitee.auth_headers(), idempotency_key=idem_key
        )

        assert job_owner["id"] != job_invitee["id"]

    async def test_jc13_idempotency_key_doesnt_bypass_concurrency(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-13: Idempotency key for existing job doesn't count against concurrency limit."""
        invitee = seed_users.invitee

        idem_key = str(uuid.uuid4())
        # Create job with idempotency key
        job1 = await create_ephemeral_job(
            http_client, invitee.auth_headers(), idempotency_key=idem_key
        )

        # Resubmit same key -> returns same job, doesn't hit limit
        job2 = await create_ephemeral_job(
            http_client, invitee.auth_headers(), idempotency_key=idem_key
        )
        assert job1["id"] == job2["id"]

    # -------------------------------------------------------------------
    # Edge Cases (JC-14 through JC-15)
    # -------------------------------------------------------------------

    async def test_jc14_canceled_job_frees_concurrency_slot(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-14: Canceled job frees concurrency slot for starter."""
        invitee = seed_users.invitee

        # Create and cancel a job
        job1 = await create_ephemeral_job(http_client, invitee.auth_headers())
        resp = await http_client.post(
            f"/v1/jobs/{job1['id']}/cancel", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # Should be able to create another
        job2 = await create_ephemeral_job(http_client, invitee.auth_headers())
        assert job2["status"] == "queued"
        assert job2["id"] != job1["id"]

    async def test_jc15_concurrent_jobs_all_visible_in_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """JC-15: All concurrent jobs visible in job list."""
        owner = seed_users.owner

        created_ids = set()
        for i in range(3):
            job = await create_ephemeral_job(
                http_client, owner.auth_headers(), spec={"prompt": f"Concurrent {i}"}
            )
            created_ids.add(job["id"])

        resp = await http_client.get("/v1/jobs", headers=owner.auth_headers())
        assert resp.status_code == 200
        listed_ids = {j["id"] for j in resp.json()}
        for jid in created_ids:
            assert jid in listed_ids
