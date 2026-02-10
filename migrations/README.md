# Database Migrations

This directory contains SQL migration files for the Framecast database schema,
managed by [sqlx-cli](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli).

## Migration Files

All migrations are **reversible** (`.up.sql` / `.down.sql` pairs).

| Migration | Description |
|-----------|-------------|
| `20240130000001_schema` | Initial schema: users, teams, memberships, invitations, api_keys, projects, generations, generation_events, asset_files, webhooks, webhook_deliveries, usage, system_assets |
| `20240130000002_functions` | Generation event sequencing, concurrency limits, status transitions, project automation, retention, URN validation |
| `20240130000003_fix_urn_validation` | Fix URN regex to accept hyphens in UUID components |
| `20250208000001_conversations_artifacts` | Add conversations, messages, artifacts, message_artifacts tables |
| `20250209000001_add_character_artifact_kind` | Add `character` to `artifact_kind` enum |
| `20250209000002_update_artifact_constraints_for_character` | Update artifact CHECK constraints for character kind |
| `20250209000003_add_key_hash_prefix` | Add `key_hash_prefix` column to api_keys for O(1) lookup |
| `20250210000001_add_attempting_delivery_status` | Add `attempting` to `webhook_delivery_status` enum |

## Running Migrations

```bash
# Run pending migrations
just migrate

# Check migration status
just migrate-status

# Create a new migration (reversible by default)
just migrate-new <name>

# Rollback last migration
just migrate-rollback
```

## Schema Overview

### Core Entities

- **users** - Application users (tier: starter/creator)
- **teams** - Team workspaces with credits and settings
- **memberships** - User-team associations with roles
- **invitations** - Pending team invitations
- **api_keys** - API authentication keys with URN-based ownership
- **projects** - Storyboard projects (team-owned)
- **generations** - AI content generations (ephemeral or project-based)
- **generation_events** - Generation progress events for SSE streaming
- **asset_files** - User-uploaded reference files
- **webhooks** - HTTP callback registrations
- **webhook_deliveries** - Webhook delivery attempt records
- **usage** - Aggregated usage metrics for billing
- **system_assets** - System-provided audio/visual assets
- **conversations** - Chat threads between user and LLM
- **messages** - Individual turns in a conversation
- **artifacts** - Creative outputs (storyboards, media, characters)

### Key Features

- **URN-based ownership** - Resources owned by users or teams via URN patterns
- **Role-based access control** - owner/admin/member/viewer roles with permissions
- **Generation concurrency limits** - Enforced at database level (CARD-5, CARD-6, INV-G12)
- **Event sequencing** - Monotonic sequence numbers for generation events (SSE support)
- **Constraint enforcement** - Business rules enforced via triggers and check constraints
