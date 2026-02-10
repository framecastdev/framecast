"""Generation Concurrency E2E Tests.

Tests concurrency limits and idempotency (15 stories):
  - Starter concurrency limits (GC-01 through GC-04)
  - Creator concurrency limits (GC-05 through GC-08)
  - Idempotency key behavior (GC-09 through GC-13)
  - Edge cases (GC-14 through GC-15)
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
)


@pytest.mark.generation_concurrency
class TestGenerationConcurrencyE2E:
    """Generation concurrency end-to-end tests."""

    # -------------------------------------------------------------------
    # Starter Concurrency Limits (GC-01 through GC-04)
    # -------------------------------------------------------------------

    async def test_gc01_starter_can_create_one_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-01: Starter user can create 1 concurrent generation."""
        invitee = seed_users.invitee

        gen = await create_ephemeral_generation(http_client, invitee.auth_headers())
        assert gen["status"] == "queued"

    async def test_gc02_starter_second_generation_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-02: Starter user's second concurrent generation is rejected (CARD-6)."""
        invitee = seed_users.invitee

        # Create first generation (stays queued)
        await create_ephemeral_generation(http_client, invitee.auth_headers())

        # Second generation should be rejected
        resp = await http_client.post(
            "/v1/generations",
            json={"spec": {"prompt": "Second generation"}},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code in [400, 409, 429], (
            f"Expected 400/409/429 for starter concurrency limit, got {resp.status_code}"
        )

    async def test_gc03_starter_can_create_after_completion(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-03: Starter can create new generation after first one completes."""
        invitee = seed_users.invitee

        # Create and complete first generation
        gen1 = await create_ephemeral_generation(http_client, invitee.auth_headers())
        await complete_generation(http_client, gen1["id"])

        # Second generation should succeed now
        gen2 = await create_ephemeral_generation(http_client, invitee.auth_headers())
        assert gen2["status"] == "queued"

    async def test_gc04_starter_can_create_after_failure(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-04: Starter can create new generation after first one fails."""
        invitee = seed_users.invitee

        # Create and fail first generation
        gen1 = await create_ephemeral_generation(http_client, invitee.auth_headers())
        await fail_generation(http_client, gen1["id"])

        # Second generation should succeed now
        gen2 = await create_ephemeral_generation(http_client, invitee.auth_headers())
        assert gen2["status"] == "queued"

    # -------------------------------------------------------------------
    # Creator Concurrency Limits (GC-05 through GC-08)
    # -------------------------------------------------------------------

    async def test_gc05_creator_can_create_multiple_concurrent_generations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-05: Creator can create multiple concurrent generations (up to 5)."""
        owner = seed_users.owner

        generations = []
        for i in range(3):
            gen = await create_ephemeral_generation(
                http_client, owner.auth_headers(), spec={"prompt": f"Generation {i}"}
            )
            generations.append(gen)
            assert gen["status"] == "queued"

        assert len(generations) == 3

    async def test_gc06_creator_can_create_up_to_five_concurrent(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-06: Creator can have up to 5 concurrent generations (CARD-5)."""
        owner = seed_users.owner

        generations = []
        for i in range(5):
            gen = await create_ephemeral_generation(
                http_client, owner.auth_headers(), spec={"prompt": f"Generation {i}"}
            )
            generations.append(gen)

        assert len(generations) == 5

    async def test_gc07_creator_sixth_generation_rejected(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-07: Creator's 6th concurrent generation is rejected (CARD-5)."""
        owner = seed_users.owner

        # Create 5 generations
        for i in range(5):
            await create_ephemeral_generation(
                http_client, owner.auth_headers(), spec={"prompt": f"Generation {i}"}
            )

        # 6th should be rejected
        resp = await http_client.post(
            "/v1/generations",
            json={"spec": {"prompt": "Generation 6"}},
            headers=owner.auth_headers(),
        )
        assert resp.status_code in [400, 409, 429], (
            f"Expected 400/409/429 for creator concurrency limit, got {resp.status_code}"
        )

    async def test_gc08_creator_can_create_after_completing_one(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-08: Creator can create generation after completing one when at limit."""
        owner = seed_users.owner

        # Create 5 generations
        generations = []
        for i in range(5):
            gen = await create_ephemeral_generation(
                http_client, owner.auth_headers(), spec={"prompt": f"Generation {i}"}
            )
            generations.append(gen)

        # Complete one
        await complete_generation(http_client, generations[0]["id"])

        # Now can create another
        new_gen = await create_ephemeral_generation(
            http_client, owner.auth_headers(), spec={"prompt": "Replacement"}
        )
        assert new_gen["status"] == "queued"

    # -------------------------------------------------------------------
    # Idempotency Key Behavior (GC-09 through GC-13)
    # -------------------------------------------------------------------

    async def test_gc09_idempotency_key_returns_same_generation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-09: Same idempotency key + same user returns existing generation."""
        owner = seed_users.owner

        idem_key = str(uuid.uuid4())
        gen1 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), idempotency_key=idem_key
        )
        gen2 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), idempotency_key=idem_key
        )

        assert gen1["id"] == gen2["id"]

    async def test_gc10_different_idempotency_keys_create_different_generations(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-10: Different idempotency keys create different generations."""
        owner = seed_users.owner

        key1 = str(uuid.uuid4())
        key2 = str(uuid.uuid4())
        gen1 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), idempotency_key=key1
        )
        gen2 = await create_ephemeral_generation(
            http_client, owner.auth_headers(), idempotency_key=key2
        )

        assert gen1["id"] != gen2["id"]

    async def test_gc11_no_idempotency_key_creates_new_generation_each_time(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-11: Without idempotency key, each request creates a new generation."""
        owner = seed_users.owner

        gen1 = await create_ephemeral_generation(http_client, owner.auth_headers())
        gen2 = await create_ephemeral_generation(http_client, owner.auth_headers())

        assert gen1["id"] != gen2["id"]

    async def test_gc12_idempotency_key_scoped_to_user(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-12: Same idempotency key from different users creates different generations."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        idem_key = str(uuid.uuid4())
        gen_owner = await create_ephemeral_generation(
            http_client, owner.auth_headers(), idempotency_key=idem_key
        )
        gen_invitee = await create_ephemeral_generation(
            http_client, invitee.auth_headers(), idempotency_key=idem_key
        )

        assert gen_owner["id"] != gen_invitee["id"]

    async def test_gc13_idempotency_key_doesnt_bypass_concurrency(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-13: Idempotency key for existing generation doesn't count against concurrency limit."""
        invitee = seed_users.invitee

        idem_key = str(uuid.uuid4())
        # Create generation with idempotency key
        gen1 = await create_ephemeral_generation(
            http_client, invitee.auth_headers(), idempotency_key=idem_key
        )

        # Resubmit same key -> returns same generation, doesn't hit limit
        gen2 = await create_ephemeral_generation(
            http_client, invitee.auth_headers(), idempotency_key=idem_key
        )
        assert gen1["id"] == gen2["id"]

    # -------------------------------------------------------------------
    # Edge Cases (GC-14 through GC-15)
    # -------------------------------------------------------------------

    async def test_gc14_canceled_generation_frees_concurrency_slot(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-14: Canceled generation frees concurrency slot for starter."""
        invitee = seed_users.invitee

        # Create and cancel a generation
        gen1 = await create_ephemeral_generation(http_client, invitee.auth_headers())
        resp = await http_client.post(
            f"/v1/generations/{gen1['id']}/cancel", headers=invitee.auth_headers()
        )
        assert resp.status_code == 200

        # Should be able to create another
        gen2 = await create_ephemeral_generation(http_client, invitee.auth_headers())
        assert gen2["status"] == "queued"
        assert gen2["id"] != gen1["id"]

    async def test_gc15_concurrent_generations_all_visible_in_list(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """GC-15: All concurrent generations visible in generation list."""
        owner = seed_users.owner

        created_ids = set()
        for i in range(3):
            gen = await create_ephemeral_generation(
                http_client, owner.auth_headers(), spec={"prompt": f"Concurrent {i}"}
            )
            created_ids.add(gen["id"])

        resp = await http_client.get("/v1/generations", headers=owner.auth_headers())
        assert resp.status_code == 200
        listed_ids = {g["id"] for g in resp.json()}
        for gid in created_ids:
            assert gid in listed_ids
