#!/bin/bash
# Check for hardcoded secrets or credentials in production code
# Excludes lines with fake/test/example passwords and pragma: allowlist secret comments

if grep -rn 'password.*=.*["\x27][^"\x27]' crates/ scripts/ --include="*.rs" --include="*.py" | grep -v "fake_password\|test_password\|example_password\|pragma: allowlist secret"; then
    echo "Found potential hardcoded secrets"
    exit 1
fi

exit 0
