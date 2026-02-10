#!/usr/bin/env python3
"""Migration file naming validation script.

Ensures migration files follow the proper naming convention.
"""

import os
import re
import sys


def validate_migration_naming(file_paths):
    """Validate migration file naming convention."""
    errors = []

    # Expected pattern: YYYYMMDDHHMMSS_description.(up|down).sql
    pattern = r"^\d{14}_[a-z0-9_]+\.(up|down)\.sql$"

    for file_path in file_paths:
        filename = os.path.basename(file_path)

        if not re.match(pattern, filename):
            errors.append(f"Invalid migration filename: {filename}")
            errors.append("  Expected format: YYYYMMDDHHMMSS_description.(up|down).sql")
            errors.append("  Example: 20240130120000_add_user_table.up.sql")

    return errors


def main():
    """Run migration naming validation as main entry point."""
    if len(sys.argv) < 2:
        return 0

    file_paths = sys.argv[1:]
    errors = validate_migration_naming(file_paths)

    if errors:
        print("Migration naming validation failed:")
        for error in errors:
            print(error)
        return 1

    print("✅ Migration naming validation passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
