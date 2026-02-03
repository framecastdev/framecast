#!/usr/bin/env python3
"""Environment variable usage validation script.

Ensures no hardcoded values and proper environment variable patterns.
"""

import re
import sys


def validate_env_vars(file_paths):
    """Validate environment variable usage."""
    errors = []

    # Patterns to detect potential hardcoded values
    suspicious_patterns = [
        (r'postgresql://[^"]*:[^"]*@[^"]*', "Hardcoded database URL"),
        (r"https://[a-zA-Z0-9-]+\.supabase\.co", "Hardcoded Supabase URL"),
        (r"sk_[a-zA-Z0-9]{32,}", "Hardcoded API key"),
        (r"AKIA[0-9A-Z]{16}", "Hardcoded AWS access key"),
        (
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            "Hardcoded UUID (potential secret)",
        ),
    ]

    # Allowed hardcoded patterns (examples, test data, etc.)
    allowed_patterns = [
        r"test@.*\.dev",  # Test emails
        r"example\.com",  # Example domains
        r"localhost",  # Local development
        r"127\.0\.0\.1",  # Local IP
        r"framecast:.*:.*",  # URN patterns
        r"usr_[a-zA-Z0-9]+",  # Test user IDs
        r"tm_[a-zA-Z0-9]+",  # Test team IDs
        r"00000000-0000-0000-0000-000000000001",  # Test UUID pattern
        r"test_.*",  # Test identifiers
        r"fake_.*",  # Fake identifiers
    ]

    for file_path in file_paths:
        try:
            with open(file_path, encoding="utf-8") as f:
                content = f.read()

            lines = content.split("\n")

            for line_num, line in enumerate(lines, 1):
                # Skip comments
                if line.strip().startswith("#") or line.strip().startswith("//"):
                    continue

                # Skip lines with allowlist pragma
                if "pragma: allowlist secret" in line:
                    continue

                for pattern, description in suspicious_patterns:
                    matches = re.finditer(pattern, line, re.IGNORECASE)
                    for match in matches:
                        matched_text = match.group()

                        # Check if it's an allowed pattern
                        is_allowed = any(
                            re.search(allowed_pattern, matched_text, re.IGNORECASE)
                            for allowed_pattern in allowed_patterns
                        )

                        if not is_allowed:
                            errors.append(f"{file_path}:{line_num}: {description}")
                            errors.append(f"  Found: {matched_text}")
                            errors.append(
                                "  Consider using environment variables instead"
                            )

        except Exception as e:
            errors.append(f"Error reading {file_path}: {e}")

    return errors


def main():
    """Run environment variable validation as main entry point."""
    if len(sys.argv) < 2:
        return 0

    file_paths = sys.argv[1:]
    errors = validate_env_vars(file_paths)

    if errors:
        print("Environment variable validation failed:")
        for error in errors:
            print(error)
        return 1

    print("✅ Environment variable validation passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
