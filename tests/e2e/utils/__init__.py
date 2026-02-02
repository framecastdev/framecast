"""
Utility modules for E2E testing.

Provides specialized tools for:
- LocalStack SES email retrieval and parsing
- Test data generation and validation
- Service mocking and setup
"""

from .localstack_email import LocalStackEmailClient

__all__ = ["LocalStackEmailClient"]
