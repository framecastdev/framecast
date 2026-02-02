#!/bin/bash
# Quick quality check hook for development workflow

set -e

echo "🔍 Running quick checks..."

# Format first
just fmt

# Quick compilation check
just clippy

# Fast unit tests only (no integration tests)
just test --lib

echo "✅ Quick checks completed"
