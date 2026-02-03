#!/usr/bin/env python3
"""Database seeding script for Framecast development.

Creates test users, teams, and sample data following the entity relationships.
"""

import asyncio
import json
import os
import sys
import uuid
from datetime import datetime, timedelta

import asyncpg

# Environment configuration
DATABASE_URL = os.getenv("DATABASE_URL")
if not DATABASE_URL:
    print("❌ DATABASE_URL environment variable is required")
    sys.exit(1)


class FramecastSeeder:
    """Database seeder for Framecast test data."""

    def __init__(self, database_url: str):
        """Initialize the seeder with database URL."""
        self.database_url = database_url
        self.conn = None

    async def connect(self):
        """Connect to the database."""
        try:
            self.conn = await asyncpg.connect(self.database_url)
            print("✅ Connected to database")
        except Exception as e:
            print(f"❌ Failed to connect to database: {e}")
            sys.exit(1)

    async def disconnect(self):
        """Disconnect from the database."""
        if self.conn:
            await self.conn.close()
            print("✅ Disconnected from database")

    async def clear_existing_data(self):
        """Clear existing test data."""
        try:
            # Clear in dependency order
            await self.conn.execute(
                "DELETE FROM job_events WHERE job_id IN "
                "(SELECT id FROM jobs WHERE owner LIKE 'framecast:%test%')"
            )
            await self.conn.execute("DELETE FROM webhook_deliveries")
            await self.conn.execute(
                "DELETE FROM jobs WHERE owner LIKE 'framecast:%test%'"
            )
            await self.conn.execute(
                "DELETE FROM asset_files WHERE owner LIKE 'framecast:%test%'"
            )
            await self.conn.execute(
                "DELETE FROM webhooks WHERE team_id IN "
                "(SELECT id FROM teams WHERE slug LIKE '%test%')"
            )
            await self.conn.execute(
                "DELETE FROM projects WHERE team_id IN "
                "(SELECT id FROM teams WHERE slug LIKE '%test%')"
            )
            await self.conn.execute(
                "DELETE FROM invitations WHERE team_id IN "
                "(SELECT id FROM teams WHERE slug LIKE '%test%')"
            )
            await self.conn.execute(
                "DELETE FROM api_keys WHERE owner LIKE 'framecast:%test%'"
            )
            await self.conn.execute(
                "DELETE FROM memberships WHERE team_id IN "
                "(SELECT id FROM teams WHERE slug LIKE '%test%')"
            )
            await self.conn.execute("DELETE FROM teams WHERE slug LIKE '%test%'")
            await self.conn.execute(
                "DELETE FROM users WHERE email LIKE '%@test.framecast.dev'"
            )

            print("✅ Cleared existing test data")
        except Exception as e:
            print(f"⚠️ Warning: Failed to clear existing data: {e}")

    async def seed_users(self) -> list[str]:
        """Create test users."""
        users = [
            {
                "id": str(uuid.uuid4()),
                "email": "alice@test.framecast.dev",
                "name": "Alice Creator",
                "tier": "creator",
                "credits": 1000,
                "upgraded_at": datetime.utcnow() - timedelta(days=30),
            },
            {
                "id": str(uuid.uuid4()),
                "email": "bob@test.framecast.dev",
                "name": "Bob Starter",
                "tier": "starter",
                "credits": 100,
                "upgraded_at": None,
            },
            {
                "id": str(uuid.uuid4()),
                "email": "charlie@test.framecast.dev",
                "name": "Charlie Creator",
                "tier": "creator",
                "credits": 500,
                "upgraded_at": datetime.utcnow() - timedelta(days=10),
            },
        ]

        user_ids = []
        for user in users:
            await self.conn.execute(
                """
                INSERT INTO users (id, email, name, tier, credits, upgraded_at)
                VALUES ($1, $2, $3, $4, $5, $6)
            """,
                user["id"],
                user["email"],
                user["name"],
                user["tier"],
                user["credits"],
                user["upgraded_at"],
            )
            user_ids.append(user["id"])

        print(f"✅ Created {len(users)} test users")
        return user_ids

    async def seed_teams(self, user_ids: list[str]) -> list[str]:
        """Create test teams."""
        teams = [
            {
                "id": str(uuid.uuid4()),
                "name": "Acme Studios Test",
                "slug": "acme-studios-test",
                "credits": 2000,
                "settings": {"notifications": {"email": True}},
                "owner_id": user_ids[0],  # Alice
            },
            {
                "id": str(uuid.uuid4()),
                "name": "Creative Lab Test",
                "slug": "creative-lab-test",
                "credits": 1500,
                "settings": {"notifications": {"email": False}},
                "owner_id": user_ids[2],  # Charlie
            },
        ]

        team_ids = []
        for team in teams:
            # Create team
            await self.conn.execute(
                """
                INSERT INTO teams (id, name, slug, credits, settings)
                VALUES ($1, $2, $3, $4, $5)
            """,
                team["id"],
                team["name"],
                team["slug"],
                team["credits"],
                json.dumps(team["settings"]),
            )

            # Add owner membership
            await self.conn.execute(
                """
                INSERT INTO memberships (id, team_id, user_id, role)
                VALUES ($1, $2, $3, 'owner')
            """,
                str(uuid.uuid4()),
                team["id"],
                team["owner_id"],
            )

            team_ids.append(team["id"])

        # Add Charlie as member of Alice's team
        await self.conn.execute(
            """
            INSERT INTO memberships (id, team_id, user_id, role)
            VALUES ($1, $2, $3, 'member')
        """,
            str(uuid.uuid4()),
            team_ids[0],
            user_ids[2],
        )

        print(f"✅ Created {len(teams)} test teams with memberships")
        return team_ids

    async def seed_projects(
        self, team_ids: list[str], user_ids: list[str]
    ) -> list[str]:
        """Create test projects."""
        projects = [
            {
                "id": str(uuid.uuid4()),
                "team_id": team_ids[0],
                "created_by": user_ids[0],
                "name": "Product Demo Video",
                "status": "draft",
                "spec": {
                    "scenes": [
                        {"type": "title", "text": "Welcome to our Product"},
                        {"type": "demo", "duration": 30},
                    ],
                    "audio": {"background": "corporate_upbeat"},
                },
            },
            {
                "id": str(uuid.uuid4()),
                "team_id": team_ids[1],
                "created_by": user_ids[2],
                "name": "Social Media Campaign",
                "status": "completed",
                "spec": {
                    "scenes": [
                        {"type": "intro", "text": "Creative Campaign"},
                        {"type": "showcase", "duration": 15},
                    ],
                    "format": {"aspect_ratio": "16:9"},
                },
            },
        ]

        project_ids = []
        for project in projects:
            await self.conn.execute(
                """
                INSERT INTO projects (id, team_id, created_by, name, status, spec)
                VALUES ($1, $2, $3, $4, $5, $6)
            """,
                project["id"],
                project["team_id"],
                project["created_by"],
                project["name"],
                project["status"],
                json.dumps(project["spec"]),
            )
            project_ids.append(project["id"])

        print(f"✅ Created {len(projects)} test projects")
        return project_ids

    async def seed_jobs(
        self, team_ids: list[str], user_ids: list[str], project_ids: list[str]
    ):
        """Create test jobs."""
        jobs = [
            {
                "id": str(uuid.uuid4()),
                "owner": f"framecast:team:{team_ids[0]}",
                "triggered_by": user_ids[0],
                "project_id": project_ids[0],
                "status": "completed",
                "spec_snapshot": {"scenes": [{"type": "title", "text": "Test"}]},
                "credits_charged": 50,
                "completed_at": datetime.utcnow() - timedelta(hours=2),
            },
            {
                "id": str(uuid.uuid4()),
                "owner": f"framecast:user:{user_ids[1]}",
                "triggered_by": user_ids[1],
                "project_id": None,  # Ephemeral job
                "status": "queued",
                "spec_snapshot": {"scenes": [{"type": "demo", "duration": 10}]},
                "credits_charged": 25,
            },
            {
                "id": str(uuid.uuid4()),
                "owner": f"framecast:team:{team_ids[1]}",
                "triggered_by": user_ids[2],
                "project_id": project_ids[1],
                "status": "failed",
                "spec_snapshot": {"scenes": [{"type": "showcase"}]},
                "credits_charged": 30,
                "credits_refunded": 30,
                "failure_type": "system",
                "error": {"message": "Test system failure"},
                "completed_at": datetime.utcnow() - timedelta(minutes=30),
            },
        ]

        for job in jobs:
            await self.conn.execute(
                """
                INSERT INTO jobs (id, owner, triggered_by, project_id, status,
                                spec_snapshot, credits_charged, credits_refunded,
                                failure_type, error, completed_at, started_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            """,
                job["id"],
                job["owner"],
                job["triggered_by"],
                job["project_id"],
                job["status"],
                json.dumps(job["spec_snapshot"]),
                job["credits_charged"],
                job.get("credits_refunded", 0),
                job.get("failure_type"),
                json.dumps(job.get("error")) if job.get("error") else None,
                job.get("completed_at"),
                job.get("started_at"),
            )

        print(f"✅ Created {len(jobs)} test jobs")

    async def seed_api_keys(self, user_ids: list[str], team_ids: list[str]):
        """Create test API keys."""
        import hashlib

        api_keys = [
            {
                "id": str(uuid.uuid4()),
                "user_id": user_ids[0],
                "owner": f"framecast:user:{user_ids[0]}",
                "name": "Development Key",
                "key_prefix": "sk_test_",
                "key_hash": hashlib.sha256(b"test_key_alice_12345").hexdigest(),
            },
            {
                "id": str(uuid.uuid4()),
                "user_id": user_ids[0],
                "owner": f"framecast:team:{team_ids[0]}",
                "name": "Team Production Key",
                "key_prefix": "sk_live_",
                "key_hash": hashlib.sha256(b"test_key_team_67890").hexdigest(),
            },
        ]

        for key in api_keys:
            await self.conn.execute(
                """
                INSERT INTO api_keys
                    (id, user_id, owner, name, key_prefix, key_hash, scopes)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
            """,
                key["id"],
                key["user_id"],
                key["owner"],
                key["name"],
                key["key_prefix"],
                key["key_hash"],
                json.dumps(["*"]),
            )

        print(f"✅ Created {len(api_keys)} test API keys")

    async def seed_system_assets(self):
        """Create test system assets."""
        system_assets = [
            {
                "id": "asset_music_corporate_upbeat",
                "category": "music",
                "name": "Corporate Upbeat",
                "description": "Energetic corporate background music",
                "duration_seconds": 120.5,
                "s3_key": "system/music/corporate_upbeat.mp3",
                "content_type": "audio/mpeg",
                "size_bytes": 2048000,
                "tags": ["corporate", "upbeat", "background"],
            },
            {
                "id": "asset_sfx_notification_bell",
                "category": "sfx",
                "name": "Notification Bell",
                "description": "Clean notification bell sound",
                "duration_seconds": 1.2,
                "s3_key": "system/sfx/notification_bell.wav",
                "content_type": "audio/wav",
                "size_bytes": 48000,
                "tags": ["notification", "bell", "alert"],
            },
            {
                "id": "asset_transition_fade_black",
                "category": "transition",
                "name": "Fade to Black",
                "description": "Smooth fade to black transition",
                "duration_seconds": 2.0,
                "s3_key": "system/transitions/fade_black.mp4",
                "content_type": "video/mp4",
                "size_bytes": 512000,
                "tags": ["fade", "black", "transition"],
            },
        ]

        for asset in system_assets:
            await self.conn.execute(
                """
                INSERT INTO system_assets
                    (id, category, name, description, duration_seconds,
                     s3_key, content_type, size_bytes, tags)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            """,
                asset["id"],
                asset["category"],
                asset["name"],
                asset["description"],
                asset["duration_seconds"],
                asset["s3_key"],
                asset["content_type"],
                asset["size_bytes"],
                asset["tags"],
            )

        print(f"✅ Created {len(system_assets)} system assets")

    async def seed_all(self, clear_existing: bool = False):
        """Seed all test data."""
        await self.connect()

        try:
            if clear_existing:
                await self.clear_existing_data()

            # Seed in dependency order
            user_ids = await self.seed_users()
            team_ids = await self.seed_teams(user_ids)
            project_ids = await self.seed_projects(team_ids, user_ids)
            await self.seed_jobs(team_ids, user_ids, project_ids)
            await self.seed_api_keys(user_ids, team_ids)
            await self.seed_system_assets()

            print("\n🌱 Database seeding completed successfully!")
            print("\nTest Data Created:")
            print("  👥 Users: alice, bob, charlie @test.framecast.dev")
            print("  🏢 Teams: acme-studios-test, creative-lab-test")
            print("  📋 Projects: Product Demo Video, Social Media Campaign")
            print("  ⚙️ Jobs: completed, queued, and failed examples")
            print("  🔑 API Keys: development and team keys")
            print("  🎵 System Assets: music, sfx, and transition examples")

        except Exception as e:
            print(f"❌ Seeding failed: {e}")
            sys.exit(1)
        finally:
            await self.disconnect()


async def main():
    """Run database seeding as main entry point."""
    import argparse

    parser = argparse.ArgumentParser(
        description="Seed Framecast database with test data"
    )
    parser.add_argument(
        "--clear", action="store_true", help="Clear existing test data before seeding"
    )

    args = parser.parse_args()

    seeder = FramecastSeeder(DATABASE_URL)
    await seeder.seed_all(clear_existing=args.clear)


if __name__ == "__main__":
    asyncio.run(main())
