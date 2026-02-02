#!/usr/bin/env python3
"""
User data export script for GDPR compliance
Exports all user data in a structured JSON format
"""

import argparse
import asyncio
import json
import os
import sys
from datetime import datetime
from typing import Any, Dict

import asyncpg

# Environment configuration
DATABASE_URL = os.getenv("DATABASE_URL")

if not DATABASE_URL:
    print("❌ DATABASE_URL environment variable is required")
    sys.exit(1)


class UserDataExporter:
    def __init__(self, database_url: str):
        self.database_url = database_url
        self.conn = None

    async def connect(self):
        """Connect to the database"""
        try:
            self.conn = await asyncpg.connect(self.database_url)
            print("✅ Connected to database")
        except Exception as e:
            print(f"❌ Failed to connect to database: {e}")
            sys.exit(1)

    async def disconnect(self):
        """Disconnect from the database"""
        if self.conn:
            await self.conn.close()
            print("✅ Disconnected from database")

    async def get_user_by_id_or_email(self, identifier: str) -> Dict:
        """Get user by ID or email"""
        # Try by UUID first
        try:
            import uuid

            uuid.UUID(identifier)
            query = "SELECT * FROM users WHERE id = $1"
            user = await self.conn.fetchrow(query, identifier)
        except ValueError:
            # Not a UUID, try by email
            query = "SELECT * FROM users WHERE email = $1"
            user = await self.conn.fetchrow(query, identifier)

        if not user:
            print(f"❌ User not found: {identifier}")
            sys.exit(1)

        return dict(user)

    async def export_user_data(self, user_id: str) -> Dict[str, Any]:
        """Export all data for a user"""
        user_data = {
            "export_timestamp": datetime.utcnow().isoformat(),
            "user_id": user_id,
            "user": None,
            "teams_owned": [],
            "teams_member": [],
            "projects_created": [],
            "jobs_triggered": [],
            "api_keys": [],
            "asset_files_uploaded": [],
            "invitations_sent": [],
            "invitations_received": [],
        }

        # Get user basic info
        user = await self.conn.fetchrow("SELECT * FROM users WHERE id = $1", user_id)
        if user:
            user_data["user"] = dict(user)

        # Get teams where user is owner
        teams_owned = await self.conn.fetch(
            """
            SELECT t.*, m.role, m.created_at as membership_created_at
            FROM teams t
            JOIN memberships m ON t.id = m.team_id
            WHERE m.user_id = $1 AND m.role = 'owner'
            ORDER BY t.created_at
        """,
            user_id,
        )
        user_data["teams_owned"] = [dict(row) for row in teams_owned]

        # Get teams where user is member (non-owner)
        teams_member = await self.conn.fetch(
            """
            SELECT t.*, m.role, m.created_at as membership_created_at
            FROM teams t
            JOIN memberships m ON t.id = m.team_id
            WHERE m.user_id = $1 AND m.role != 'owner'
            ORDER BY t.created_at
        """,
            user_id,
        )
        user_data["teams_member"] = [dict(row) for row in teams_member]

        # Get projects created by user
        projects = await self.conn.fetch(
            """
            SELECT p.*, t.name as team_name, t.slug as team_slug
            FROM projects p
            JOIN teams t ON p.team_id = t.id
            WHERE p.created_by = $1
            ORDER BY p.created_at
        """,
            user_id,
        )
        user_data["projects_created"] = [dict(row) for row in projects]

        # Get jobs triggered by user
        jobs = await self.conn.fetch(
            """
            SELECT j.*, p.name as project_name
            FROM jobs j
            LEFT JOIN projects p ON j.project_id = p.id
            WHERE j.triggered_by = $1
            ORDER BY j.created_at DESC
        """,
            user_id,
        )
        user_data["jobs_triggered"] = [dict(row) for row in jobs]

        # Get API keys owned by user
        api_keys = await self.conn.fetch(
            """
            SELECT id, owner, name, key_prefix, scopes, last_used_at,
                   expires_at, revoked_at, created_at
            FROM api_keys
            WHERE user_id = $1
            ORDER BY created_at
        """,
            user_id,
        )
        user_data["api_keys"] = [dict(row) for row in api_keys]

        # Get asset files uploaded by user
        assets = await self.conn.fetch(
            """
            SELECT af.*, p.name as project_name
            FROM asset_files af
            LEFT JOIN projects p ON af.project_id = p.id
            WHERE af.uploaded_by = $1
            ORDER BY af.created_at
        """,
            user_id,
        )
        user_data["asset_files_uploaded"] = [dict(row) for row in assets]

        # Get invitations sent by user
        invitations_sent = await self.conn.fetch(
            """
            SELECT i.*, t.name as team_name, t.slug as team_slug
            FROM invitations i
            JOIN teams t ON i.team_id = t.id
            WHERE i.invited_by = $1
            ORDER BY i.created_at
        """,
            user_id,
        )
        user_data["invitations_sent"] = [dict(row) for row in invitations_sent]

        # Get invitations received by user (by email)
        if user:
            user_email = user["email"]
            invitations_received = await self.conn.fetch(
                """
                SELECT i.*, t.name as team_name, t.slug as team_slug,
                       u.name as invited_by_name, u.email as invited_by_email
                FROM invitations i
                JOIN teams t ON i.team_id = t.id
                JOIN users u ON i.invited_by = u.id
                WHERE i.email = $1
                ORDER BY i.created_at
            """,
                user_email,
            )
            user_data["invitations_received"] = [
                dict(row) for row in invitations_received
            ]

        return user_data

    async def export_to_file(self, user_identifier: str, output_file: str = None):
        """Export user data to JSON file"""
        print(f"📤 Exporting data for user: {user_identifier}")

        # Get user and validate
        user = await self.get_user_by_id_or_email(user_identifier)
        user_id = user["id"]
        user_email = user["email"]

        print(f"✅ Found user: {user['name']} ({user_email})")

        # Export all data
        user_data = await self.export_user_data(user_id)

        # Determine output file
        if not output_file:
            timestamp = datetime.utcnow().strftime("%Y%m%d_%H%M%S")
            safe_email = user_email.replace("@", "_at_").replace(".", "_")
            output_file = f"user_export_{safe_email}_{timestamp}.json"

        # Write to file
        with open(output_file, "w") as f:
            json.dump(user_data, f, indent=2, default=str)

        print(f"✅ User data exported to: {output_file}")

        # Print summary
        print("\n📊 Export Summary:")
        print(f"  Teams owned: {len(user_data['teams_owned'])}")
        print(f"  Teams member: {len(user_data['teams_member'])}")
        print(f"  Projects created: {len(user_data['projects_created'])}")
        print(f"  Jobs triggered: {len(user_data['jobs_triggered'])}")
        print(f"  API keys: {len(user_data['api_keys'])}")
        print(f"  Asset files: {len(user_data['asset_files_uploaded'])}")
        print(f"  Invitations sent: {len(user_data['invitations_sent'])}")
        print(f"  Invitations received: {len(user_data['invitations_received'])}")

        return output_file


async def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(description="Export user data for GDPR compliance")
    parser.add_argument("user", help="User ID (UUID) or email address")
    parser.add_argument("--output", "-o", help="Output file path (optional)")

    args = parser.parse_args()

    exporter = UserDataExporter(DATABASE_URL)

    try:
        await exporter.connect()
        await exporter.export_to_file(args.user, args.output)
    finally:
        await exporter.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
