# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# Framecast API

Storyboard-to-video generation API. Converts YAML/JSON specs → videos via ComfyUI.

---

<law>
## CRITICAL RULES — READ FIRST

These rules are NON-NEGOTIABLE. Violations require immediate correction.

### Rule 1: Just is the ONLY Entry Point

**YOU MUST** use Just targets for ALL tasks. NEVER run commands directly.

```bash
# ✅ CORRECT — Always use Just
just build
just test
just migrate

# ❌ FORBIDDEN — Never bypass Just
cargo build          # WRONG
pytest               # WRONG
uv run pytest        # WRONG
```

If a Just target doesn't exist: **CREATE IT FIRST**, then run `just <task>`.

### Rule 2: Tests Before Code

**YOU MUST** brainstorm test cases BEFORE writing implementation code.
Cover: happy path, edge cases, error conditions, invariants.
See `TEST_STRATEGY.md` for examples.

### Rule 3: No .unwrap() in Production

**NEVER** use `.unwrap()` or `.expect()` in production Rust code.
Use `?` operator or proper error handling.

### Rule 4: Config via Environment Only

**NEVER** hardcode credentials, URLs, or configuration.
All config comes from environment variables.

### Rule 5: Stateless Processes

**NEVER** store state in memory between requests.
All persistent data goes to database or S3.

### Rule 6: Libraries First

**YOU MUST** leverage third-party libraries before implementing custom solutions.
Assess quality: GitHub stars (>500), recent releases, no known CVEs.

### Rule 7: Feature Branch Workflow

**YOU MUST** always work on feature/fix branches. NEVER commit directly to main.
Break problems into phases/tasks with one branch per task.

```bash
# ✅ CORRECT
git checkout -b feature/add-webhook-retries
# implement, test with just test && just check
git commit -m "feat: add webhook retry logic"
```

</law>

---

## Build & Test Commands

```bash
# Essential commands
just              # Show all commands
just setup        # First-time setup (install all tools)
just dev          # Start local environment (LocalStack, Inngest, PostgreSQL)
just test         # Run all Rust tests
just check        # Run all quality checks (fmt, clippy, tests, pre-commit)

# Running specific tests
just test domain              # Test specific crate
just test "test_name"         # Test matching pattern
just test-watch               # Run tests with file watching

# E2E tests (Python)
just test-e2e-mocked          # Fast E2E tests (CI-friendly)
just test-e2e-with-email      # E2E with LocalStack email verification
just test-invitation-workflow # Complete invitation flow

# Database
just migrate                  # Run pending migrations
just migrate-new <name>       # Create new migration
just migrate-status           # Check migration status
just sqlx-prepare             # Generate offline query data

# Code quality
just fmt                      # Format all code
just clippy                   # Run linter
just fix                      # Auto-fix linting issues
just ci                       # Run full CI pipeline locally
just precommit-install        # Install pre-commit hooks

# Build & Deploy
just lambda-build             # Build Lambda with cargo-lambda
just lambda-watch             # Hot reload for local Lambda dev
just deploy-local             # Deploy full stack to LocalStack
just deploy-dev               # Deploy to AWS dev
just deploy-prod              # Deploy to AWS production

# Infrastructure
just infra-init               # Initialize OpenTofu
just infra-validate           # Validate OpenTofu config
just infra-plan dev           # Plan changes for environment
```

### Offline Development

When developing without database access, use:

```bash
SQLX_OFFLINE=true cargo clippy   # Or: just ci-clippy
```

---

## Local Development Services

`just dev` starts:

| Service     | Port  | Purpose                    |
|-------------|-------|----------------------------|
| API         | 3000  | Framecast API server       |
| PostgreSQL  | 5432  | Database                   |
| LocalStack  | 4566  | AWS S3/Lambda emulation    |
| Inngest     | 8288  | Job orchestration UI       |

Additional commands:

- `just health-check` - Verify all services are running
- `just logs` - View aggregated service logs
- `just stop` - Stop all services

---

## Architecture

### Crate Structure

```
crates/
├── api/          # Lambda handlers, HTTP routes (axum)
├── domain/       # Business logic, entities, validation, state machines
├── db/           # Database layer, repositories (sqlx + PostgreSQL)
├── email/        # AWS SES email service
├── inngest/      # Job orchestration client
├── comfyui/      # RunPod/ComfyUI client for video generation
└── common/       # Shared utilities, error types, URN parsing
```

### Dependency Flow

```
api → domain → common
 ↓      ↓
db   email/inngest/comfyui
```

- `domain` contains pure business logic, no I/O
- `api` orchestrates handlers and injects dependencies
- `db`, `email`, `inngest`, `comfyui` are backing service adapters

### Key Patterns

**Error Handling:** `thiserror` for library crates, `anyhow` for application code.

**Repository Pattern:** Database access via trait-based repositories in `db/` crate, injected into handlers.

**State Machines:** Job/Project/Invitation states defined in `domain/src/state.rs` with explicit transitions.

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| API | Rust + Lambda (axum) |
| Database | Supabase (PostgreSQL) |
| Auth | Supabase Auth |
| Orchestration | Inngest |
| Video | RunPod + ComfyUI |
| AI/LLM | Anthropic Claude |
| Storage | S3 / LocalStack |
| IaC | OpenTofu |

---

## Specification Reference

The `docs/spec/` directory contains the formal API specification:

| File | Purpose |
|------|---------|
| `04_Entities.md` | Database entities, field definitions |
| `05_Relationships_States.md` | State machines (Job, Project, Invitation) |
| `06_Invariants.md` | Business rules that MUST be enforced |
| `07_Operations.md` | API endpoint specifications |
| `08_Permissions.md` | Role-based access control matrix |

### Domain Model

**User Tiers:** Visitor → Starter → Creator

**Core Entities:** User, Team, Membership, Project, Job, AssetFile, Webhook, ApiKey

**Job States:** `queued → processing → completed/failed/canceled`

### Key Invariants

- Every team has ≥1 owner (INV-T2)
- Only creators can have team memberships (INV-M4)
- Max 1 active job per project (INV-J12)
- Credits cannot go negative (INV-U5, INV-T6)
- Max 5 concurrent jobs per team (CARD-5)

---

## Reference Projects

**Location:** `~/workspace/splice-experimental-1`, `~/workspace/splice-experimental-2`

Consult for proven patterns: Rust + Lambda, database schemas, testing patterns,
ComfyUI/RunPod integration, Inngest job orchestration.

---

## Skills

The `.claude/skills/` directory contains domain expertise modules:

| Skill | When to Use |
|-------|-------------|
| `api-spec` | API operations, permissions, validation |
| `rust-patterns` | Error handling, repository pattern, handlers |
| `python-e2e` | pytest fixtures, async patterns, mocking |
| `runpod-infra` | Docker images, GPU workload, ComfyUI |
| `observability` | Structured logging, metrics, tracing |
