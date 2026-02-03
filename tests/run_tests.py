#!/usr/bin/env python3
"""
Test runner for Framecast - Rule 2 Compliance
Comprehensive test execution with proper reporting

Usage:
    python tests/run_tests.py --all              # Run all tests
    python tests/run_tests.py --unit             # Run unit tests only
    python tests/run_tests.py --integration      # Run integration tests only
    python tests/run_tests.py --migrations       # Run migration tests only
    python tests/run_tests.py --admin-scripts    # Run admin script tests only
    python tests/run_tests.py --performance      # Run performance tests only
"""

import argparse
import os
import subprocess
import sys


class FramecastTestRunner:
    """Comprehensive test runner for Framecast"""

    def __init__(self):
        self.project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        os.chdir(self.project_root)

    def run_command(self, command: list[str]) -> int:
        """Run command and return exit code"""
        print(f"Running: {' '.join(command)}")
        result = subprocess.run(command, cwd=self.project_root)
        return result.returncode

    def install_test_dependencies(self) -> int:
        """Install test dependencies"""
        print("📦 Installing test dependencies...")
        return self.run_command(
            [sys.executable, "-m", "pip", "install", "-r", "tests/requirements.txt"]
        )

    def run_migration_tests(self) -> int:
        """Run database migration tests"""
        print("🗃️ Running database migration tests...")
        return self.run_command(
            [
                "python",
                "-m",
                "pytest",
                "tests/migrations/",
                "-v",
                "--tb=short",
                "-m",
                "not slow",
            ]
        )

    def run_admin_script_tests(self) -> int:
        """Run admin script tests"""
        print("⚙️ Running admin script tests...")
        return self.run_command(
            [
                "python",
                "-m",
                "pytest",
                "tests/admin_scripts/",
                "-v",
                "--tb=short",
                "-m",
                "not slow",
            ]
        )

    def run_integration_tests(self) -> int:
        """Run integration tests"""
        print("🔗 Running integration tests...")
        return self.run_command(
            [
                "python",
                "-m",
                "pytest",
                "tests/",
                "-v",
                "--tb=short",
                "-m",
                "integration",
            ]
        )

    def run_performance_tests(self) -> int:
        """Run performance tests"""
        print("🚀 Running performance tests...")
        return self.run_command(
            [
                "python",
                "-m",
                "pytest",
                "tests/",
                "-v",
                "--tb=short",
                "-m",
                "performance",
            ]
        )

    def run_unit_tests(self) -> int:
        """Run Rust unit tests"""
        print("🦀 Running Rust unit tests...")
        return self.run_command(["cargo", "test", "--workspace"])

    def run_all_tests(self) -> int:
        """Run comprehensive test suite"""
        print("🧪 Running comprehensive test suite...")

        # Install dependencies first
        if self.install_test_dependencies() != 0:
            print("❌ Failed to install test dependencies")
            return 1

        test_results = []

        # Run all test categories
        test_categories = [
            ("Unit Tests", self.run_unit_tests),
            ("Migration Tests", self.run_migration_tests),
            ("Admin Script Tests", self.run_admin_script_tests),
            ("Integration Tests", self.run_integration_tests),
        ]

        for name, test_func in test_categories:
            print(f"\n{'=' * 50}")
            print(f"Running {name}")
            print("=" * 50)

            result = test_func()
            test_results.append((name, result))

            if result != 0:
                print(f"❌ {name} failed with exit code {result}")
            else:
                print(f"✅ {name} passed")

        # Summary
        print(f"\n{'=' * 50}")
        print("TEST SUMMARY")
        print("=" * 50)

        failed_tests = []
        for name, result in test_results:
            status = "✅ PASSED" if result == 0 else "❌ FAILED"
            print(f"{status}: {name}")
            if result != 0:
                failed_tests.append(name)

        if failed_tests:
            print(f"\n❌ {len(failed_tests)} test category(ies) failed:")
            for test in failed_tests:
                print(f"  - {test}")
            return 1
        print(f"\n✅ All {len(test_results)} test categories passed!")
        return 0

    def run_coverage_report(self) -> int:
        """Generate test coverage report"""
        print("📊 Generating test coverage report...")

        # Python test coverage
        python_coverage = self.run_command(
            [
                "python",
                "-m",
                "pytest",
                "tests/",
                "--cov=scripts",
                "--cov-report=html:coverage_html",
                "--cov-report=term-missing",
            ]
        )

        # Rust test coverage would use cargo-tarpaulin if available

        if python_coverage == 0:
            print("✅ Coverage report generated in coverage_html/")

        return python_coverage


def main():
    parser = argparse.ArgumentParser(description="Run Framecast tests")
    parser.add_argument("--all", action="store_true", help="Run all tests")
    parser.add_argument("--unit", action="store_true", help="Run unit tests only")
    parser.add_argument(
        "--integration", action="store_true", help="Run integration tests"
    )
    parser.add_argument("--migrations", action="store_true", help="Run migration tests")
    parser.add_argument(
        "--admin-scripts", action="store_true", help="Run admin script tests"
    )
    parser.add_argument(
        "--performance", action="store_true", help="Run performance tests"
    )
    parser.add_argument(
        "--coverage", action="store_true", help="Generate coverage report"
    )
    parser.add_argument(
        "--install-deps", action="store_true", help="Install test dependencies"
    )

    args = parser.parse_args()

    runner = FramecastTestRunner()

    if args.install_deps:
        return runner.install_test_dependencies()
    if args.unit:
        return runner.run_unit_tests()
    if args.migrations:
        return runner.run_migration_tests()
    if args.admin_scripts:
        return runner.run_admin_script_tests()
    if args.integration:
        return runner.run_integration_tests()
    if args.performance:
        return runner.run_performance_tests()
    if args.coverage:
        return runner.run_coverage_report()
    if args.all:
        return runner.run_all_tests()
    # Default: run basic test suite
    return runner.run_all_tests()


if __name__ == "__main__":
    sys.exit(main())
