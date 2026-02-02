# Framecast Admin Scripts

This directory contains operational scripts for the Framecast API following 12-Factor Rule XII (Admin Processes).

## Scripts Overview

### `seed.py` - Database Seeding

Seeds the database with test data for development.

```bash
just seed
# or directly:
python scripts/seed.py --clear  # Clear existing test data first
```

**Creates:**

- Test users (starter and creator tiers)
- Teams with proper memberships
- Sample projects and jobs
- API keys for testing
- System assets (music, SFX, transitions)

### `cleanup_jobs.py` - Job Cleanup

Removes old job records and associated files based on retention policies.

```bash
just cleanup-jobs days=30
# or directly:
python scripts/cleanup_jobs.py --days 30 --dry-run
python scripts/cleanup_jobs.py --days 90 --max-jobs 1000
```

**Features:**

- Configurable retention period (minimum 7 days)
- Dry-run mode for safety
- S3 object cleanup
- Batch processing limits
- Comprehensive statistics

### `export_user_data.py` - GDPR Data Export

Exports all user data in structured JSON format for GDPR compliance.

```bash
just export-user-data alice@test.framecast.dev
# or directly:
python scripts/export_user_data.py alice@test.framecast.dev -o export.json
python scripts/export_user_data.py usr_12345  # By user ID
```

**Exports:**

- User profile and settings
- Team memberships and ownership
- Projects, jobs, and assets
- API keys (metadata only)
- Invitation history

### `generate_api_key.py` - API Key Generation

Creates new API keys for admin use with proper URN validation.

```bash
just generate-api-key "Production Key"
# or directly:
python scripts/generate_api_key.py "My Key" --user alice@test.framecast.dev
python scripts/generate_api_key.py "Team Key" --user alice@test.framecast.dev --owner framecast:team:tm_123
python scripts/generate_api_key.py --list-teams --user alice@test.framecast.dev
```

**Features:**

- URN-based ownership validation
- Team key generation for creator users
- Configurable scopes and expiration
- Secure key generation with proper hashing

### `health_check.py` - Service Health Monitoring

Checks the health of all backing services.

```bash
just health-check
# or directly:
python scripts/health_check.py --json
python scripts/health_check.py --exit-code  # For monitoring scripts
```

**Checks:**

- PostgreSQL database connectivity and migrations
- LocalStack S3 buckets and access
- Inngest service health
- External API reachability

## Dependencies

All scripts require:

- Python 3.8+
- `asyncpg` - PostgreSQL async driver
- `aiohttp` - HTTP client for service checks
- `boto3` - AWS SDK for S3 operations

Install with:

```bash
pip install asyncpg aiohttp boto3
# or use uv (recommended):
uv add asyncpg aiohttp boto3
```

## Environment Variables

Scripts use the same environment variables as the main application:

```bash
# Database
DATABASE_URL=postgresql://postgres:password@localhost:5432/framecast_dev

# AWS/LocalStack
AWS_REGION=us-east-1
S3_BUCKET_OUTPUTS=framecast-outputs-dev
S3_BUCKET_ASSETS=framecast-assets-dev
LOCALSTACK_ENDPOINT=http://localhost:4566

# Inngest
INNGEST_ENDPOINT=http://localhost:8288
```

## Usage Patterns

### Development Workflow

```bash
# Start environment
just dev

# Check everything is healthy
just health-check

# Seed test data
just seed

# Run tests
just test-e2e-mocked
```

### Maintenance Tasks

```bash
# Weekly cleanup (dry-run first)
python scripts/cleanup_jobs.py --days 30 --dry-run
python scripts/cleanup_jobs.py --days 30

# Export user data for GDPR request
just export-user-data user@company.com

# Generate production API key
python scripts/generate_api_key.py "Production API" --user admin@framecast.com --owner framecast:team:tm_prod --expires 365
```

### Monitoring

```bash
# Basic health check
just health-check

# JSON output for monitoring systems
python scripts/health_check.py --json --exit-code
```

## Security Notes

- **API Keys**: Generated keys are only shown once and cannot be retrieved
- **Database Access**: Scripts require full database access - use carefully in production
- **S3 Operations**: Cleanup scripts can delete data - always test with dry-run first
- **User Data**: Export scripts include all user data - handle according to privacy policies

## Error Handling

All scripts include comprehensive error handling:

- Database connection failures
- Service unavailability
- Invalid parameters
- Permission issues
- Rate limiting

Use `--help` with any script for detailed usage information.
