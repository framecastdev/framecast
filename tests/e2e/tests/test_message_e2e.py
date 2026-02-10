"""Message E2E Tests.

Tests message send/list operations (24 stories):
  - Core flow (MS01-MS07)
  - List & ordering (MS08-MS10)
  - Error conditions (MS11-MS13)
  - Ownership (MS14-MS15)
  - Auth (MS16-MS17)
  - Edge cases (MS18-MS24)
"""

import sys
import uuid
from pathlib import Path

sys.path.append(str(Path(__file__).parent.parent))

import httpx  # noqa: E402
import pytest  # noqa: E402
from conftest import (  # noqa: E402
    SeededUsers,
    create_conversation,
    send_message,
)


@pytest.mark.messages
class TestMessageE2E:
    """Message send/list end-to-end tests."""

    # -----------------------------------------------------------------------
    # Core Flow (MS01-MS07)
    # -----------------------------------------------------------------------

    async def test_ms01_send_message_returns_user_and_assistant(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS01: Send message -> response with user_message + assistant_message."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(http_client, owner.auth_headers(), conv["id"])

        assert "user_message" in result
        assert "assistant_message" in result

    async def test_ms02_user_message_has_role_user(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS02: user_message has role=user."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(http_client, owner.auth_headers(), conv["id"])

        assert result["user_message"]["role"] == "user"

    async def test_ms03_assistant_message_has_role_assistant(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS03: assistant_message has role=assistant."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(http_client, owner.auth_headers(), conv["id"])

        assert result["assistant_message"]["role"] == "assistant"

    async def test_ms04_first_send_sequences_1_2(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS04: First send: sequences 1, 2."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(http_client, owner.auth_headers(), conv["id"])

        assert result["user_message"]["sequence"] == 1
        assert result["assistant_message"]["sequence"] == 2

    async def test_ms05_second_send_sequences_3_4(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS05: Second send: sequences 3, 4."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"], "First")
        result = await send_message(
            http_client, owner.auth_headers(), conv["id"], "Second"
        )

        assert result["user_message"]["sequence"] == 3
        assert result["assistant_message"]["sequence"] == 4

    async def test_ms06_after_send_message_count_is_2(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS06: After send, conversation.message_count=2."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["message_count"] == 2

    async def test_ms07_after_send_last_message_at_not_null(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS07: After send, last_message_at not null and recent."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}", headers=owner.auth_headers()
        )
        assert resp.status_code == 200
        assert resp.json()["last_message_at"] is not None

    # -----------------------------------------------------------------------
    # List & Ordering (MS08-MS10)
    # -----------------------------------------------------------------------

    async def test_ms08_list_messages_ordered_by_sequence(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS08: List messages -> ordered by sequence ASC."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"], "Hello")

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        messages = resp.json()
        sequences = [m["sequence"] for m in messages]
        assert sequences == sorted(sequences)

    async def test_ms09_send_twice_list_returns_four(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS09: Send twice, list -> 4 messages, seq 1-4."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"], "First")
        await send_message(http_client, owner.auth_headers(), conv["id"], "Second")

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        messages = resp.json()
        assert len(messages) == 4
        sequences = [m["sequence"] for m in messages]
        assert sequences == [1, 2, 3, 4]

    async def test_ms10_new_conversation_list_empty(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS10: New conversation, list messages -> []."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        assert resp.json() == []

    # -----------------------------------------------------------------------
    # Error Conditions (MS11-MS13)
    # -----------------------------------------------------------------------

    async def test_ms11_send_to_archived_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS11: Send to archived conversation -> 400."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        # Archive it
        await http_client.patch(
            f"/v1/conversations/{conv['id']}",
            json={"status": "archived"},
            headers=owner.auth_headers(),
        )

        # Try to send
        resp = await http_client.post(
            f"/v1/conversations/{conv['id']}/messages",
            json={"content": "Hello"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for archived conv, got {resp.status_code} {resp.text}"
        )

    async def test_ms12_send_empty_content(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS12: Empty content -> 400."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.post(
            f"/v1/conversations/{conv['id']}/messages",
            json={"content": ""},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for empty content, got {resp.status_code} {resp.text}"
        )

    async def test_ms13_send_whitespace_only_content(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS13: Whitespace-only content -> 400."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.post(
            f"/v1/conversations/{conv['id']}/messages",
            json={"content": "   \t\n  "},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 400, (
            f"Expected 400 for whitespace content, got {resp.status_code} {resp.text}"
        )

    # -----------------------------------------------------------------------
    # Ownership (MS14-MS15)
    # -----------------------------------------------------------------------

    async def test_ms14_invitee_cannot_send_to_owners_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS14: Invitee sends to owner's conversation -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.post(
            f"/v1/conversations/{conv['id']}/messages",
            json={"content": "Hello"},
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 404

    async def test_ms15_invitee_cannot_list_owners_messages(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS15: Invitee lists owner's messages -> 404."""
        owner = seed_users.owner
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, owner.auth_headers())

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=invitee.auth_headers(),
        )
        assert resp.status_code == 404

    # -----------------------------------------------------------------------
    # Auth (MS16-MS17)
    # -----------------------------------------------------------------------

    async def test_ms16_send_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS16: Send no auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            f"/v1/conversations/{fake_id}/messages",
            json={"content": "Hello"},
        )
        assert resp.status_code == 401

    async def test_ms17_list_no_auth_returns_401(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS17: List no auth -> 401."""
        fake_id = str(uuid.uuid4())
        resp = await http_client.get(
            f"/v1/conversations/{fake_id}/messages",
        )
        assert resp.status_code == 401

    # -----------------------------------------------------------------------
    # Edge Cases (MS18-MS24)
    # -----------------------------------------------------------------------

    async def test_ms18_send_to_nonexistent_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS18: Send to nonexistent conversation -> 404."""
        owner = seed_users.owner

        fake_id = str(uuid.uuid4())
        resp = await http_client.post(
            f"/v1/conversations/{fake_id}/messages",
            json={"content": "Hello"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404

    async def test_ms19_assistant_message_model_not_null(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS19: assistant_message.model is not null."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(http_client, owner.auth_headers(), conv["id"])

        assert result["assistant_message"]["model"] is not None

    async def test_ms20_assistant_message_has_token_counts(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS20: assistant_message has input_tokens + output_tokens."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        result = await send_message(http_client, owner.auth_headers(), conv["id"])

        assistant = result["assistant_message"]
        assert assistant["input_tokens"] is not None
        assert assistant["output_tokens"] is not None

    async def test_ms21_message_has_all_fields(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS21: Each message has all expected fields."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        await send_message(http_client, owner.auth_headers(), conv["id"])

        resp = await http_client.get(
            f"/v1/conversations/{conv['id']}/messages",
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 200
        messages = resp.json()

        expected_fields = [
            "id",
            "conversation_id",
            "role",
            "content",
            "sequence",
            "created_at",
        ]
        for msg in messages:
            for field in expected_fields:
                assert field in msg, f"Missing field '{field}' in message: {msg}"

    async def test_ms22_long_content_accepted(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS22: 50000-char content -> success (text has no max)."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        long_content = "x" * 50000
        result = await send_message(
            http_client, owner.auth_headers(), conv["id"], long_content
        )
        assert result["user_message"]["content"] == long_content

    async def test_ms23_starter_can_send_message(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS23: Starter user sends message -> success."""
        invitee = seed_users.invitee

        conv = await create_conversation(http_client, invitee.auth_headers())
        result = await send_message(http_client, invitee.auth_headers(), conv["id"])

        assert result["user_message"]["role"] == "user"
        assert result["assistant_message"]["role"] == "assistant"

    async def test_ms24_send_to_deleted_conversation(
        self,
        http_client: httpx.AsyncClient,
        seed_users: SeededUsers,
    ):
        """MS24: Send to deleted conversation -> 404."""
        owner = seed_users.owner

        conv = await create_conversation(http_client, owner.auth_headers())
        conv_id = conv["id"]

        # Delete the conversation
        resp = await http_client.delete(
            f"/v1/conversations/{conv_id}", headers=owner.auth_headers()
        )
        assert resp.status_code == 204

        # Try to send
        resp = await http_client.post(
            f"/v1/conversations/{conv_id}/messages",
            json={"content": "Hello"},
            headers=owner.auth_headers(),
        )
        assert resp.status_code == 404
