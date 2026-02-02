# Database Migrations

This directory contains SQL migration files for the Framecast database schema.

## Migration Files

- `001_initial_schema.sql` - Initial database schema with all core entities
- `002_job_sequence_functions.sql` - Job event sequencing and constraint functions

## Running Migrations

Use the Just command to run migrations:

```bash
just migrate
```

Or run manually with psql:

```bash
psql $DATABASE_URL -f migrations/001_initial_schema.sql
psql $DATABASE_URL -f migrations/002_job_sequence_functions.sql
```

## Schema Overview

### Core Entities

- **users** - Application users (tier: starter/creator)
- **teams** - Team workspaces with credits and settings
- **memberships** - User-team associations with roles
- **invitations** - Pending team invitations
- **api_keys** - API authentication keys with URN-based ownership
- **projects** - Storyboard projects (team-owned)
- **jobs** - Video generation jobs (ephemeral or project-based)
- **job_events** - Job progress events for SSE streaming
- **asset_files** - User-uploaded reference files
- **webhooks** - HTTP callback registrations
- **webhook_deliveries** - Webhook delivery attempt records
- **usage** - Aggregated usage metrics for billing
- **system_assets** - System-provided audio/visual assets

### Key Features

- **URN-based ownership** - Resources owned by users or teams via URN patterns
- **Role-based access control** - owner/admin/member/viewer roles with permissions
- **Job concurrency limits** - Enforced at database level (CARD-5, CARD-6, INV-J12)
- **Automatic status transitions** - Project status updates based on job states
- **Event sequencing** - Monotonic sequence numbers for job events (SSE support)
- **Constraint enforcement** - Business rules enforced via triggers and check constraints
- **Retention policies** - Automatic cleanup functions for old events/deliveries

### Business Rules Enforced

- Users cannot have negative credits (INV-U5, INV-T6)
- Starter users cannot have team memberships (INV-U3)
- Teams must have ≥1 owner and ≥1 member (INV-T1, INV-T2)
- Only creator users can have team API keys
- Max 1 active job per project (INV-J12)
- Max 5 concurrent jobs per team (CARD-5)
- Max 1 concurrent job per starter user (CARD-6)
- Credits refunded cannot exceed credits charged (INV-J8)
- Job status transitions follow state machine rules

### Performance Optimizations

- Indexes on all foreign keys and frequently queried columns
- GIN indexes for JSONB and array columns
- Partial indexes for performance-critical queries
- Efficient cleanup functions with row count limits

## Development Notes

- All entities have `created_at` and most have `updated_at` with automatic triggers
- UUIDs used for all primary keys
- JSONB used for flexible schema fields (spec, options, progress, metadata)
- Enum types for controlled vocabularies (status, role, tier, etc.)
- Comprehensive check constraints for data integrity
- Foreign key constraints with appropriate CASCADE behaviors
