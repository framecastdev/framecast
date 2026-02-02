#!/usr/bin/env python3
"""
API specification validation script
Ensures API specification files are consistent and well-formed
"""

import os
import re
import sys


def validate_api_spec(file_paths):
    """Validate API specification consistency"""
    errors = []

    for file_path in file_paths:
        try:
            with open(file_path, encoding="utf-8") as f:
                content = f.read()

            # Check for common specification issues
            lines = content.split("\n")

            for line_num, line in enumerate(lines, 1):
                # Check for placeholder text that should be replaced
                if "TODO" in line or "FIXME" in line:
                    errors.append(f"{file_path}:{line_num}: Contains TODO/FIXME")

                # Check for broken internal links
                link_pattern = r"\[([^\]]+)\]\(([^)]+)\)"
                for match in re.finditer(link_pattern, line):
                    link_text = match.group(1)
                    link_url = match.group(2)

                    # Check internal markdown links
                    if link_url.endswith(".md") and not link_url.startswith("http"):
                        # Resolve relative path
                        spec_dir = os.path.dirname(file_path)
                        target_path = os.path.join(spec_dir, link_url)

                        if not os.path.exists(target_path):
                            errors.append(
                                f"{file_path}:{line_num}: Broken link to {link_url}"
                            )

                # Check for consistent heading format
                if line.startswith("#"):
                    if not re.match(r"^#+\s+\S", line):
                        errors.append(
                            f"{file_path}:{line_num}: Heading should have space after #"
                        )

        except Exception as e:
            errors.append(f"Error reading {file_path}: {e}")

    return errors


def main():
    if len(sys.argv) < 2:
        return 0

    file_paths = sys.argv[1:]
    errors = validate_api_spec(file_paths)

    if errors:
        print("API specification validation failed:")
        for error in errors:
            print(error)
        return 1

    print("✅ API specification validation passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
