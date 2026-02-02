#!/usr/bin/env python3
"""Job cleanup script for Framecast.

Removes old job records and associated files based on configurable retention policies.
"""

import argparse
import asyncio
import os
import sys
from datetime import datetime, timedelta

import asyncpg
import boto3

# Environment configuration
DATABASE_URL = os.getenv("DATABASE_URL")
AWS_REGION = os.getenv("AWS_REGION", "us-east-1")
S3_BUCKET_OUTPUTS = os.getenv("S3_BUCKET_OUTPUTS")

if not DATABASE_URL:
    print("❌ DATABASE_URL environment variable is required")
    sys.exit(1)


class JobCleanupService:
    """Service for cleaning up old job records and associated files."""

    def __init__(self, database_url: str, dry_run: bool = False):
        """Initialize the job cleanup service.

        Args:
            database_url: PostgreSQL connection URL
            dry_run: If True, only report what would be deleted without deleting
        """
        self.database_url = database_url
        self.dry_run = dry_run
        self.conn = None
        self.s3_client = None

    async def connect(self):
        """Connect to database and AWS services."""
        try:
            self.conn = await asyncpg.connect(self.database_url)
            print("✅ Connected to database")

            # Initialize S3 client if bucket is configured
            if S3_BUCKET_OUTPUTS:
                self.s3_client = boto3.client("s3", region_name=AWS_REGION)
                print("✅ Connected to S3")
        except Exception as e:
            print(f"❌ Failed to connect: {e}")
            sys.exit(1)

    async def disconnect(self):
        """Disconnect from database."""
        if self.conn:
            await self.conn.close()
            print("✅ Disconnected from database")

    async def get_old_jobs(self, days_old: int) -> list[dict]:
        """Get jobs older than specified days."""
        cutoff_date = datetime.utcnow() - timedelta(days=days_old)

        query = """
            SELECT j.id, j.owner, j.status, j.output, j.created_at,
                   j.output_size_bytes, j.credits_charged, j.credits_refunded,
                   COUNT(je.id) as event_count
            FROM jobs j
            LEFT JOIN job_events je ON j.id = je.job_id
            WHERE j.created_at < $1
                AND j.status IN ('completed', 'failed', 'canceled')
            GROUP BY j.id
            ORDER BY j.created_at ASC
        """

        rows = await self.conn.fetch(query, cutoff_date)
        return [dict(row) for row in rows]

    async def cleanup_job_events(self, job_ids: list[str]) -> int:
        """Clean up job events for given jobs."""
        if not job_ids:
            return 0

        if self.dry_run:
            count_query = """
                SELECT COUNT(*) FROM job_events WHERE job_id = ANY($1::uuid[])
            """
            count = await self.conn.fetchval(count_query, job_ids)
            print(f"  📝 Would delete {count} job events (DRY RUN)")
            return count

        delete_query = """
            DELETE FROM job_events WHERE job_id = ANY($1::uuid[])
        """
        result = await self.conn.execute(delete_query, job_ids)
        deleted_count = int(result.split()[-1])
        print(f"  🗑️ Deleted {deleted_count} job events")
        return deleted_count

    async def cleanup_webhook_deliveries(self, job_ids: list[str]) -> int:
        """Clean up webhook deliveries for given jobs."""
        if not job_ids:
            return 0

        if self.dry_run:
            count_query = """
                SELECT COUNT(*) FROM webhook_deliveries WHERE job_id = ANY($1::uuid[])
            """
            count = await self.conn.fetchval(count_query, job_ids)
            print(f"  📝 Would delete {count} webhook deliveries (DRY RUN)")
            return count

        delete_query = """
            DELETE FROM webhook_deliveries WHERE job_id = ANY($1::uuid[])
        """
        result = await self.conn.execute(delete_query, job_ids)
        deleted_count = int(result.split()[-1])
        print(f"  🗑️ Deleted {deleted_count} webhook deliveries")
        return deleted_count

    def get_s3_keys_from_output(self, output: dict) -> list[str]:
        """Extract S3 keys from job output."""
        keys = []
        if not output:
            return keys

        # Look for common S3 key patterns in output
        if "video_url" in output:
            url = output["video_url"]
            if "s3://" in url:
                # Extract key from s3:// URL
                key = url.replace(f"s3://{S3_BUCKET_OUTPUTS}/", "")
                keys.append(key)

        # Look for asset keys
        if "assets" in output:
            for asset in output.get("assets", []):
                if "s3_key" in asset:
                    keys.append(asset["s3_key"])

        return keys

    async def cleanup_s3_objects(self, jobs: list[dict]) -> int:
        """Clean up S3 objects for jobs."""
        if not self.s3_client or not S3_BUCKET_OUTPUTS:
            print("  ⚠️ S3 cleanup skipped (not configured)")
            return 0

        s3_keys = []
        for job in jobs:
            if job.get("output"):
                keys = self.get_s3_keys_from_output(job["output"])
                s3_keys.extend(keys)

        if not s3_keys:
            print("  ✅ No S3 objects to clean up")
            return 0

        if self.dry_run:
            print(f"  📝 Would delete {len(s3_keys)} S3 objects (DRY RUN)")
            for key in s3_keys[:5]:  # Show first 5
                print(f"    - {key}")
            if len(s3_keys) > 5:
                print(f"    ... and {len(s3_keys) - 5} more")
            return len(s3_keys)

        # Delete S3 objects in batches
        deleted_count = 0
        batch_size = 1000  # S3 delete limit

        for i in range(0, len(s3_keys), batch_size):
            batch = s3_keys[i : i + batch_size]

            delete_objects = {
                "Objects": [{"Key": key} for key in batch],
                "Quiet": True
            }

            try:
                response = self.s3_client.delete_objects(
                    Bucket=S3_BUCKET_OUTPUTS, Delete=delete_objects
                )

                batch_deleted = len(response.get("Deleted", []))
                deleted_count += batch_deleted

                errors = response.get("Errors", [])
                for error in errors:
                    print(
                        f"    ⚠️ Failed to delete {error['Key']}: {error['Message']}"
                    )

            except Exception as e:
                print(f"    ❌ S3 batch delete failed: {e}")

        print(f"  🗑️ Deleted {deleted_count} S3 objects")
        return deleted_count

    async def delete_jobs(self, job_ids: list[str]) -> int:
        """Delete job records."""
        if not job_ids:
            return 0

        if self.dry_run:
            print(f"  📝 Would delete {len(job_ids)} job records (DRY RUN)")
            return len(job_ids)

        delete_query = """
            DELETE FROM jobs WHERE id = ANY($1::uuid[])
        """
        result = await self.conn.execute(delete_query, job_ids)
        deleted_count = int(result.split()[-1])
        print(f"  🗑️ Deleted {deleted_count} job records")
        return deleted_count

    async def cleanup_old_jobs(self, days_old: int, max_jobs: int = None):
        """Execute cleanup routine for old jobs."""
        prefix = "DRY RUN: " if self.dry_run else ""
        print(f"🧹 {prefix}Cleaning up jobs older than {days_old} days...")

        # Get old jobs
        old_jobs = await self.get_old_jobs(days_old)

        if not old_jobs:
            print("✅ No old jobs found to clean up")
            return

        # Limit if specified
        if max_jobs and len(old_jobs) > max_jobs:
            print(f"  📊 Found {len(old_jobs)} old jobs, limiting to {max_jobs}")
            old_jobs = old_jobs[:max_jobs]
        else:
            print(f"  📊 Found {len(old_jobs)} old jobs to clean up")

        # Calculate statistics
        total_size = sum(job.get("output_size_bytes", 0) for job in old_jobs)
        total_credits = sum(
            job.get("credits_charged", 0) - job.get("credits_refunded", 0)
            for job in old_jobs
        )

        print(f"  💰 Total net credits: {total_credits}")
        print(f"  📦 Total storage size: {total_size / 1024 / 1024:.2f} MB")

        # Group by status for reporting
        by_status = {}
        for job in old_jobs:
            status = job["status"]
            by_status[status] = by_status.get(status, 0) + 1

        for status, count in by_status.items():
            print(f"  📈 {status}: {count} jobs")

        job_ids = [job["id"] for job in old_jobs]

        # Clean up in dependency order
        stats = {"jobs": len(old_jobs), "events": 0, "deliveries": 0, "s3_objects": 0}

        print(
            f"\n{'🔍 Analyzing' if self.dry_run else '🗑️ Cleaning up'} associated data:"
        )

        # 1. Clean up job events
        stats["events"] = await self.cleanup_job_events(job_ids)

        # 2. Clean up webhook deliveries
        stats["deliveries"] = await self.cleanup_webhook_deliveries(job_ids)

        # 3. Clean up S3 objects
        stats["s3_objects"] = await self.cleanup_s3_objects(old_jobs)

        # 4. Delete job records (this will cascade to remaining FK relationships)
        stats["jobs"] = await self.delete_jobs(job_ids)

        print(f"\n{'📋 Summary (DRY RUN)' if self.dry_run else '✅ Cleanup Summary'}:")
        print(f"  Jobs: {stats['jobs']}")
        print(f"  Events: {stats['events']}")
        print(f"  Webhook deliveries: {stats['deliveries']}")
        print(f"  S3 objects: {stats['s3_objects']}")
        print(f"  Storage freed: {total_size / 1024 / 1024:.2f} MB")


async def main():
    """Execute the cleanup script."""
    parser = argparse.ArgumentParser(description="Clean up old Framecast job data")
    parser.add_argument(
        "--days",
        type=int,
        default=30,
        help="Delete jobs older than this many days (default: 30)",
    )
    parser.add_argument(
        "--max-jobs", type=int, help="Maximum number of jobs to process in one run"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be deleted without actually deleting",
    )

    args = parser.parse_args()

    if args.days < 7:
        print("❌ Minimum retention period is 7 days for safety")
        sys.exit(1)

    cleanup_service = JobCleanupService(DATABASE_URL, dry_run=args.dry_run)

    try:
        await cleanup_service.connect()
        await cleanup_service.cleanup_old_jobs(args.days, args.max_jobs)
    finally:
        await cleanup_service.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
