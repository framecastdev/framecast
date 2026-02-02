# Claude Hooks for Framecast API

This directory contains Claude-specific configuration and hooks to streamline
the development workflow.

## Available Hooks

### 🎨 format-code.sh

Format all code using Just targets instead of direct cargo calls.

- **Usage**: `.claude/hooks/format-code.sh` or alias `fmt`
- **What it does**: Runs `just fmt` to format all Rust code
- **Why**: Maintains consistency with project workflow, avoids direct cargo calls

### 🔍 quick-check.sh

Quick quality check for development workflow.

- **Usage**: `.claude/hooks/quick-check.sh` or alias `check`
- **What it does**: Format + clippy + unit tests
- **Why**: Fast feedback during development

### ✅ pre-commit.sh

Complete pre-commit validation.

- **Usage**: `.claude/hooks/pre-commit.sh`
- **What it does**: Format + clippy + all tests
- **Why**: Ensure code quality before commits

## Project Workflow Philosophy

This project follows **"Just is the Frontend"** principle (Critical Rule #1):

- ✅ Use `just fmt` instead of `cargo fmt --all`
- ✅ Use `just test` instead of `cargo test --workspace`
- ✅ Use `just check` instead of `cargo clippy --workspace`

## Claude Settings

The `settings.json` file configures Claude to:

1. **Prefer Just commands** over direct cargo calls
2. **Use project hooks** for common operations
3. **Follow 12-factor principles** and project rules

This ensures consistency with the project's development standards and avoids
external command dependencies.
