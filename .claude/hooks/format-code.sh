#!/bin/bash
# Code formatting hook that uses Just instead of direct cargo calls

set -e

echo "🎨 Formatting code..."

# Use Just for formatting to maintain consistency with project workflow
just fmt

echo "✅ Code formatting completed"
