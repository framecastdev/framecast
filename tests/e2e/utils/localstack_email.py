"""
LocalStack SES Email Client

Provides functionality to retrieve and parse emails sent through LocalStack SES
for comprehensive E2E testing of email workflows.
"""

import re
import asyncio
from typing import Dict, List, Optional, Any
from datetime import datetime

import httpx


class LocalStackEmail:
    """Represents an email retrieved from LocalStack SES."""

    def __init__(self, data: Dict[str, Any]):
        """Initialize email from LocalStack response data."""
        self.id = data.get("id", "")
        self.subject = data.get("subject", "")
        self.body = data.get("body", "")
        self.from_address = data.get("from", "")
        self.to = data.get("to", [])
        self.timestamp = data.get("timestamp", "")
        self.raw_data = data

    def __repr__(self) -> str:
        return f"LocalStackEmail(id='{self.id}', subject='{self.subject}', to={self.to})"


class LocalStackEmailClient:
    """Client for retrieving emails from LocalStack SES REST API."""

    def __init__(self, base_url: str = "http://localhost:4566"):
        """
        Initialize LocalStack email client.

        Args:
            base_url: LocalStack base URL (default: http://localhost:4566)
        """
        self.base_url = base_url
        self.client = httpx.AsyncClient(timeout=30.0)

    async def close(self) -> None:
        """Close the HTTP client."""
        await self.client.aclose()

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()

    async def get_emails(self, email_address: str) -> List[LocalStackEmail]:
        """
        Get all emails for a specific email address.

        Args:
            email_address: Email address to retrieve emails for

        Returns:
            List of LocalStackEmail objects

        Raises:
            httpx.RequestError: If request fails
            httpx.HTTPStatusError: If HTTP status indicates error
        """
        url = f"{self.base_url}/_aws/ses"

        try:
            response = await self.client.get(url, params={"email": email_address})
            response.raise_for_status()

            data = response.json()

            # Handle case where LocalStack returns different response formats
            if isinstance(data, list):
                emails = data
            elif isinstance(data, dict) and "emails" in data:
                emails = data["emails"]
            else:
                # If response format is unexpected, try to adapt
                emails = [data] if data else []

            return [LocalStackEmail(email) for email in emails]

        except httpx.RequestError as e:
            raise httpx.RequestError(f"Failed to retrieve emails from LocalStack: {e}")
        except httpx.HTTPStatusError as e:
            raise httpx.HTTPStatusError(
                f"LocalStack SES API error: {e.response.status_code}",
                request=e.request,
                response=e.response
            )

    async def get_latest_email(self, email_address: str) -> Optional[LocalStackEmail]:
        """
        Get the most recent email for an email address.

        Args:
            email_address: Email address to check

        Returns:
            Most recent LocalStackEmail or None if no emails found
        """
        emails = await self.get_emails(email_address)
        if not emails:
            return None

        # Sort by timestamp (if available) or by ID as fallback
        def sort_key(email: LocalStackEmail) -> str:
            if email.timestamp:
                try:
                    # Try to parse timestamp for proper sorting
                    dt = datetime.fromisoformat(email.timestamp.replace('Z', '+00:00'))
                    return dt.isoformat()
                except ValueError:
                    # Fall back to string comparison
                    return email.timestamp
            return email.id

        sorted_emails = sorted(emails, key=sort_key, reverse=True)
        return sorted_emails[0]

    async def get_latest_invitation(self, email_address: str) -> Optional[LocalStackEmail]:
        """
        Get the most recent invitation email for an email address.

        Args:
            email_address: Email address to check

        Returns:
            Most recent invitation email or None if not found
        """
        emails = await self.get_emails(email_address)

        # Filter for invitation emails
        invitation_emails = [
            email for email in emails
            if "invitation" in email.subject.lower() or "invite" in email.subject.lower()
        ]

        if not invitation_emails:
            return None

        # Sort by timestamp and return most recent
        def sort_key(email: LocalStackEmail) -> str:
            if email.timestamp:
                try:
                    dt = datetime.fromisoformat(email.timestamp.replace('Z', '+00:00'))
                    return dt.isoformat()
                except ValueError:
                    return email.timestamp
            return email.id

        sorted_emails = sorted(invitation_emails, key=sort_key, reverse=True)
        return sorted_emails[0]

    async def delete_email(self, message_id: str) -> bool:
        """
        Delete a specific email by message ID.

        Args:
            message_id: LocalStack email message ID

        Returns:
            True if deletion successful, False otherwise
        """
        url = f"{self.base_url}/_aws/ses"

        try:
            response = await self.client.delete(url, params={"id": message_id})
            return response.status_code == 200
        except (httpx.RequestError, httpx.HTTPStatusError):
            return False

    async def clear_emails(self, email_address: str) -> int:
        """
        Clear all emails for a specific email address.

        Args:
            email_address: Email address to clear

        Returns:
            Number of emails deleted
        """
        emails = await self.get_emails(email_address)
        deleted_count = 0

        for email in emails:
            if await self.delete_email(email.id):
                deleted_count += 1

        return deleted_count

    def extract_invitation_url(self, email_body: str) -> Optional[str]:
        """
        Extract invitation acceptance URL from email content.

        Args:
            email_body: HTML or text email content

        Returns:
            Invitation URL if found, None otherwise
        """
        patterns = [
            # Full URL patterns
            r'https?://[^/]+/teams/[^/]+/invitations/([^/\s\'"]+)/accept',
            r'https?://framecast\.app/teams/[^/]+/invitations/([^/\s\'"]+)/accept',

            # Relative URL patterns
            r'/teams/[^/]+/invitations/([^/\s\'"]+)/accept',

            # General invitation URL patterns
            r'invitation[_\-]?url["\s]*[:=]["\s]*([^"\s]+)',
            r'accept[_\-]?invitation["\s]*[:=]["\s]*([^"\s]+)',

            # Button/link href patterns
            r'href=["\'](.*?invitations.*?accept.*?)["\'"]',
        ]

        for pattern in patterns:
            match = re.search(pattern, email_body, re.IGNORECASE)
            if match:
                url = match.group(1) if len(match.groups()) > 0 else match.group(0)
                # Clean up the URL if needed
                if url.startswith('/'):
                    return f"https://framecast.app{url}"
                elif url.startswith('http'):
                    return url
                else:
                    # Assume it's a relative path
                    return f"https://framecast.app/{url.lstrip('/')}"

        return None

    def extract_invitation_id(self, email_body: str) -> Optional[str]:
        """
        Extract invitation ID (UUID) from email content.

        Args:
            email_body: HTML or text email content

        Returns:
            Invitation UUID if found, None otherwise
        """
        # UUID v4 pattern
        uuid_pattern = r'[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}'

        patterns = [
            # In invitation URLs
            rf'/invitations/({uuid_pattern})/accept',
            rf'invitation[_\-]?id["\s]*[:=]["\s]*["\']?({uuid_pattern})["\']?',
            rf'invitations/({uuid_pattern})',

            # In query parameters
            rf'invitation_id=({uuid_pattern})',
            rf'id=({uuid_pattern})',

            # In metadata
            rf'"invitation_id"["\s]*:["\s]*"({uuid_pattern})"',
            rf'invitation_id["\s]*=["\s]*["\']({uuid_pattern})["\']',
        ]

        for pattern in patterns:
            match = re.search(pattern, email_body, re.IGNORECASE)
            if match:
                return match.group(1)

        return None

    def extract_team_id(self, email_body: str) -> Optional[str]:
        """
        Extract team ID (UUID) from email content.

        Args:
            email_body: HTML or text email content

        Returns:
            Team UUID if found, None otherwise
        """
        # UUID v4 pattern
        uuid_pattern = r'[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}'

        patterns = [
            # In team URLs
            rf'/teams/({uuid_pattern})/',
            rf'/teams/({uuid_pattern})/invitations',
            rf'team[_\-]?id["\s]*[:=]["\s]*["\']?({uuid_pattern})["\']?',

            # In query parameters
            rf'team_id=({uuid_pattern})',

            # In metadata
            rf'"team_id"["\s]*:["\s]*"({uuid_pattern})"',
            rf'team_id["\s]*=["\s]*["\']({uuid_pattern})["\']',
        ]

        for pattern in patterns:
            match = re.search(pattern, email_body, re.IGNORECASE)
            if match:
                return match.group(1)

        return None

    async def wait_for_email(
        self,
        email_address: str,
        timeout: float = 10.0,
        poll_interval: float = 0.5
    ) -> Optional[LocalStackEmail]:
        """
        Wait for an email to arrive at the specified address.

        Args:
            email_address: Email address to monitor
            timeout: Maximum time to wait in seconds
            poll_interval: Time between checks in seconds

        Returns:
            The latest email if received within timeout, None otherwise
        """
        start_time = asyncio.get_event_loop().time()

        while (asyncio.get_event_loop().time() - start_time) < timeout:
            email = await self.get_latest_email(email_address)
            if email:
                return email

            await asyncio.sleep(poll_interval)

        return None

    async def wait_for_invitation_email(
        self,
        email_address: str,
        timeout: float = 10.0,
        poll_interval: float = 0.5
    ) -> Optional[LocalStackEmail]:
        """
        Wait for an invitation email to arrive at the specified address.

        Args:
            email_address: Email address to monitor
            timeout: Maximum time to wait in seconds
            poll_interval: Time between checks in seconds

        Returns:
            The latest invitation email if received within timeout, None otherwise
        """
        start_time = asyncio.get_event_loop().time()

        while (asyncio.get_event_loop().time() - start_time) < timeout:
            email = await self.get_latest_invitation(email_address)
            if email:
                return email

            await asyncio.sleep(poll_interval)

        return None