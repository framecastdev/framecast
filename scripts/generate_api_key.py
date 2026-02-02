#!/usr/bin/env python3
"""
API key generation script for Framecast
Creates new API keys for admin use with proper URN validation
"""

import asyncio
import asyncpg
import secrets
import hashlib
import os
import sys
import json
from typing import List
import argparse

# Environment configuration
DATABASE_URL = os.getenv('DATABASE_URL')

if not DATABASE_URL:
    print("❌ DATABASE_URL environment variable is required")
    sys.exit(1)

class APIKeyGenerator:
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

    def generate_api_key(self, prefix: str = "sk_dev") -> tuple[str, str]:
        """Generate a new API key with hash"""
        # Generate 32 bytes of random data
        key_data = secrets.token_urlsafe(32)
        full_key = f"{prefix}_{key_data}"

        # Create hash for storage
        key_hash = hashlib.sha256(full_key.encode()).hexdigest()

        return full_key, key_hash

    async def get_user_by_email(self, email: str) -> dict:
        """Get user by email"""
        query = "SELECT * FROM users WHERE email = $1"
        user = await self.conn.fetchrow(query, email)

        if not user:
            print(f"❌ User not found: {email}")
            sys.exit(1)

        return dict(user)

    async def get_user_teams(self, user_id: str) -> List[dict]:
        """Get teams where user is owner or admin"""
        query = """
            SELECT t.id, t.name, t.slug, m.role
            FROM teams t
            JOIN memberships m ON t.id = m.team_id
            WHERE m.user_id = $1 AND m.role IN ('owner', 'admin')
            ORDER BY t.name
        """
        rows = await self.conn.fetch(query, user_id)
        return [dict(row) for row in rows]

    async def validate_urn_ownership(self, user: dict, owner_urn: str) -> bool:
        """Validate that user can own the given URN"""
        user_id = user['id']
        user_tier = user['tier']

        # Personal URN - any user can own
        if owner_urn == f"framecast:user:{user_id}":
            return True

        # Starter users can only have personal URNs
        if user_tier == 'starter':
            return False

        # Creator users can have team URNs if they're owner/admin
        if owner_urn.startswith('framecast:team:'):
            team_id = owner_urn.replace('framecast:team:', '')
            teams = await self.get_user_teams(user_id)
            return any(team['id'] == team_id for team in teams)

        # Team-user URNs (framecast:tm_xxx:usr_yyy)
        if ':' in owner_urn.replace('framecast:', ''):
            parts = owner_urn.split(':')
            if len(parts) == 3:
                team_part = parts[1]
                if team_part.startswith('tm_'):
                    team_id = team_part[3:]  # Remove tm_ prefix
                    teams = await self.get_user_teams(user_id)
                    return any(team['id'] == team_id for team in teams)

        return False

    async def create_api_key(self, name: str, owner_urn: str, user_email: str,
                           scopes: List[str] = None, expires_days: int = None) -> dict:
        """Create a new API key"""
        # Get user
        user = await self.get_user_by_email(user_email)

        # Validate URN ownership
        if not await self.validate_urn_ownership(user, owner_urn):
            print(f"❌ User cannot create API key for URN: {owner_urn}")
            print(f"   User tier: {user['tier']}")
            if user['tier'] == 'creator':
                teams = await self.get_user_teams(user['id'])
                print(f"   Available teams: {[t['name'] for t in teams]}")
            sys.exit(1)

        # Determine key prefix based on environment/URN type
        if 'team' in owner_urn:
            prefix = "sk_live"  # Team keys are production
        else:
            prefix = "sk_dev"   # Personal keys are development

        # Generate key
        full_key, key_hash = self.generate_api_key(prefix)

        # Set default scopes
        if not scopes:
            scopes = ["*"]

        # Calculate expiration if specified
        expires_at = None
        if expires_days:
            from datetime import datetime, timedelta
            expires_at = datetime.utcnow() + timedelta(days=expires_days)

        # Insert into database
        api_key_id = str(await self.conn.fetchval(
            "SELECT gen_random_uuid()"
        ))

        await self.conn.execute("""
            INSERT INTO api_keys (id, user_id, owner, name, key_prefix, key_hash, scopes, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        """, api_key_id, user['id'], owner_urn, name, prefix,
            key_hash, json.dumps(scopes), expires_at)

        print(f"✅ API key created successfully")
        print(f"   ID: {api_key_id}")
        print(f"   Owner: {owner_urn}")
        print(f"   Name: {name}")
        print(f"   Scopes: {scopes}")
        if expires_at:
            print(f"   Expires: {expires_at.isoformat()}")

        return {
            'id': api_key_id,
            'key': full_key,
            'owner': owner_urn,
            'name': name,
            'scopes': scopes,
            'expires_at': expires_at
        }

async def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(description='Generate Framecast API key')
    parser.add_argument('name', help='Name for the API key')
    parser.add_argument('--user', '-u', required=True,
                      help='User email who will own the key')
    parser.add_argument('--owner', '-o',
                      help='Owner URN (defaults to user personal URN)')
    parser.add_argument('--scopes', '-s',
                      help='Comma-separated list of scopes (defaults to "*")')
    parser.add_argument('--expires', '-e', type=int,
                      help='Expiration in days (optional)')
    parser.add_argument('--list-teams', action='store_true',
                      help='List available teams for the user')

    args = parser.parse_args()

    generator = APIKeyGenerator(DATABASE_URL)

    try:
        await generator.connect()

        # Get user first
        user = await generator.get_user_by_email(args.user)

        if args.list_teams:
            teams = await generator.get_user_teams(user['id'])
            print(f"\n📋 Available teams for {user['name']} ({user['email']}):")
            if teams:
                for team in teams:
                    print(f"   🏢 {team['name']} ({team['slug']}) - {team['role']}")
                    print(f"       URN: framecast:team:{team['id']}")
            else:
                print("   (No teams available)")
            return

        # Determine owner URN
        owner_urn = args.owner or f"framecast:user:{user['id']}"

        # Parse scopes
        scopes = None
        if args.scopes:
            scopes = [s.strip() for s in args.scopes.split(',')]

        # Create API key
        api_key = await generator.create_api_key(
            args.name, owner_urn, args.user, scopes, args.expires
        )

        print(f"\n🔑 API Key Generated:")
        print(f"   {api_key['key']}")
        print(f"\n⚠️  IMPORTANT: Save this key now - it cannot be retrieved again!")

    finally:
        await generator.disconnect()

if __name__ == '__main__':
    asyncio.run(main())