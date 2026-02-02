#!/usr/bin/env python3
"""Health check script for Framecast backing services.

Verifies connectivity and basic functionality of all required services.
"""

import asyncio
import json
import os
import sys
from datetime import datetime

import aiohttp
import asyncpg
import boto3

# Environment configuration
DATABASE_URL = os.getenv("DATABASE_URL")
LOCALSTACK_ENDPOINT = os.getenv("LOCALSTACK_ENDPOINT", "http://localhost:4566")
INNGEST_ENDPOINT = os.getenv("INNGEST_ENDPOINT", "http://localhost:8288")
S3_BUCKET_OUTPUTS = os.getenv("S3_BUCKET_OUTPUTS", "framecast-outputs-dev")
S3_BUCKET_ASSETS = os.getenv("S3_BUCKET_ASSETS", "framecast-assets-dev")
AWS_REGION = os.getenv("AWS_REGION", "us-east-1")


class HealthChecker:
    def __init__(self):
        self.results = {
            "timestamp": datetime.utcnow().isoformat(),
            "overall_status": "unknown",
            "services": {},
        }

    async def check_database(self) -> bool:
        """Check PostgreSQL database connectivity"""
        print("🗃️ Checking database...")

        if not DATABASE_URL:
            print("  ❌ DATABASE_URL not configured")
            self.results["services"]["database"] = {
                "status": "error",
                "message": "DATABASE_URL not configured",
            }
            return False

        try:
            conn = await asyncpg.connect(DATABASE_URL)

            # Test basic query
            result = await conn.fetchval("SELECT 1")

            # Check migration status
            try:
                migrations = await conn.fetch(
                    "SELECT * FROM _sqlx_migrations ORDER BY version"
                )
                migration_count = len(migrations)
            except:
                migration_count = 0

            await conn.close()

            print(f"  ✅ Database connected ({migration_count} migrations applied)")
            self.results["services"]["database"] = {
                "status": "healthy",
                "migrations_applied": migration_count,
                "connection_url": (
                    DATABASE_URL.split("@")[1] if "@" in DATABASE_URL else "configured"
                ),
            }
            return True

        except Exception as e:
            print(f"  ❌ Database connection failed: {e}")
            self.results["services"]["database"] = {
                "status": "error",
                "message": str(e),
            }
            return False

    async def check_localstack(self) -> bool:
        """Check LocalStack S3 services"""
        print("🪣 Checking LocalStack...")

        try:
            # Use LocalStack endpoint
            s3_client = boto3.client(
                "s3",
                endpoint_url=LOCALSTACK_ENDPOINT,
                region_name=AWS_REGION,
                aws_access_key_id="test",
                aws_secret_access_key="test",
            )

            # Check if buckets exist
            buckets = s3_client.list_buckets()
            bucket_names = [b["Name"] for b in buckets["Buckets"]]

            outputs_exists = S3_BUCKET_OUTPUTS in bucket_names
            assets_exists = S3_BUCKET_ASSETS in bucket_names

            # Test bucket access
            can_write = False
            if outputs_exists:
                try:
                    s3_client.put_object(
                        Bucket=S3_BUCKET_OUTPUTS,
                        Key="health-check.txt",
                        Body=b"health check test",
                    )
                    s3_client.delete_object(
                        Bucket=S3_BUCKET_OUTPUTS, Key="health-check.txt"
                    )
                    can_write = True
                except Exception as e:
                    print(f"    ⚠️ Cannot write to bucket: {e}")

            status = (
                "healthy"
                if (outputs_exists and assets_exists and can_write)
                else "degraded"
            )

            print(f"  {'✅' if status == 'healthy' else '⚠️'} LocalStack S3")
            print(
                f"    Outputs bucket: {'✅' if outputs_exists else '❌'} {S3_BUCKET_OUTPUTS}"
            )
            print(
                f"    Assets bucket: {'✅' if assets_exists else '❌'} {S3_BUCKET_ASSETS}"
            )
            print(f"    Write access: {'✅' if can_write else '❌'}")

            self.results["services"]["localstack"] = {
                "status": status,
                "endpoint": LOCALSTACK_ENDPOINT,
                "buckets": {
                    "outputs": {"name": S3_BUCKET_OUTPUTS, "exists": outputs_exists},
                    "assets": {"name": S3_BUCKET_ASSETS, "exists": assets_exists},
                },
                "write_access": can_write,
            }

            return status == "healthy"

        except Exception as e:
            print(f"  ❌ LocalStack connection failed: {e}")
            self.results["services"]["localstack"] = {
                "status": "error",
                "message": str(e),
            }
            return False

    async def check_inngest(self) -> bool:
        """Check Inngest service"""
        print("⚙️ Checking Inngest...")

        try:
            async with aiohttp.ClientSession() as session:
                # Check Inngest health endpoint
                async with session.get(f"{INNGEST_ENDPOINT}/health") as response:
                    if response.status == 200:
                        health_data = await response.json()
                        print("  ✅ Inngest healthy")

                        self.results["services"]["inngest"] = {
                            "status": "healthy",
                            "endpoint": INNGEST_ENDPOINT,
                            "health_data": health_data,
                        }
                        return True
                    print(f"  ❌ Inngest health check failed: {response.status}")

                    self.results["services"]["inngest"] = {
                        "status": "error",
                        "message": f"Health endpoint returned {response.status}",
                    }
                    return False

        except Exception as e:
            print(f"  ❌ Inngest connection failed: {e}")
            self.results["services"]["inngest"] = {"status": "error", "message": str(e)}
            return False

    async def check_external_apis(self) -> bool:
        """Check external API connectivity (without credentials)"""
        print("🌐 Checking external APIs...")

        external_status = True

        # Anthropic API (just check if endpoint is reachable)
        try:
            async with (
                aiohttp.ClientSession() as session,
                session.get("https://api.anthropic.com", timeout=5) as response,
            ):
                # Any response (even 401) means the service is reachable
                print("  ✅ Anthropic API reachable")
        except Exception as e:
            print(f"  ⚠️ Anthropic API unreachable: {e}")
            external_status = False

        # RunPod API (just check if endpoint is reachable)
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get("https://api.runpod.ai", timeout=5) as response:
                    print("  ✅ RunPod API reachable")
        except Exception as e:
            print(f"  ⚠️ RunPod API unreachable: {e}")
            external_status = False

        status = "healthy" if external_status else "degraded"
        self.results["services"]["external_apis"] = {
            "status": status,
            "message": "Basic connectivity check only (no auth verification)",
        }

        return external_status

    async def run_all_checks(self) -> dict:
        """Run all health checks"""
        print("🏥 Running Framecast health checks...\n")

        checks = [
            self.check_database(),
            self.check_localstack(),
            self.check_inngest(),
            self.check_external_apis(),
        ]

        results = await asyncio.gather(*checks, return_exceptions=True)

        # Calculate overall status
        healthy_count = sum(1 for result in results if result is True)
        total_checks = len(results)

        if healthy_count == total_checks:
            overall_status = "healthy"
            status_emoji = "✅"
        elif healthy_count > total_checks // 2:
            overall_status = "degraded"
            status_emoji = "⚠️"
        else:
            overall_status = "unhealthy"
            status_emoji = "❌"

        self.results["overall_status"] = overall_status

        print(f"\n🏥 Overall Health: {status_emoji} {overall_status.upper()}")
        print(f"   Services healthy: {healthy_count}/{total_checks}")

        return self.results

    def print_detailed_report(self):
        """Print detailed health report"""
        print("\n📋 Detailed Health Report:")
        print(f"   Timestamp: {self.results['timestamp']}")
        print(f"   Overall Status: {self.results['overall_status']}")
        print("\n   Service Details:")

        for service_name, details in self.results["services"].items():
            status_emoji = {
                "healthy": "✅",
                "degraded": "⚠️",
                "error": "❌",
                "unknown": "❓",
            }.get(details["status"], "❓")

            print(f"     {status_emoji} {service_name}: {details['status']}")

            if "message" in details:
                print(f"        Message: {details['message']}")


async def main():
    """Main entry point"""
    import argparse

    parser = argparse.ArgumentParser(description="Check Framecast service health")
    parser.add_argument(
        "--json", action="store_true", help="Output results in JSON format"
    )
    parser.add_argument(
        "--exit-code",
        action="store_true",
        help="Exit with non-zero code if any service is unhealthy",
    )

    args = parser.parse_args()

    checker = HealthChecker()
    results = await checker.run_all_checks()

    if args.json:
        print(json.dumps(results, indent=2))
    else:
        checker.print_detailed_report()

    if args.exit_code:
        if results["overall_status"] == "healthy":
            sys.exit(0)
        elif results["overall_status"] == "degraded":
            sys.exit(1)
        else:
            sys.exit(2)


if __name__ == "__main__":
    asyncio.run(main())
