#!/bin/bash
# Pre-commit hook to format code and run checks
# This avoids external cargo calls by using Just targets

set -e

echo "🎨 Running pre-commit hooks..."

# Format code using Just
just fmt

# Run basic checks
just clippy

# Run tests
just test

echo "✅ Pre-commit checks passed"
