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
npm run build        # WRONG
pytest               # WRONG
uv run pytest        # WRONG
```

If a Just target doesn't exist: **CREATE IT FIRST**, then run `just <task>`.

### Rule 2: Tests Before Code

**YOU MUST** brainstorm test cases BEFORE writing implementation code.
Cover: happy path, edge cases, error conditions, invariants.

**Reference:** See `TEST_STRATEGY.md` for comprehensive test planning examples.

### Rule 3: No .unwrap() in Production

**NEVER** use `.unwrap()` or `.expect()` in production Rust code.
Use `?` operator or proper error handling.

### Rule 4: Config via Environment Only

**NEVER** hardcode credentials, URLs, or configuration.
All config comes from environment variables.

### Rule 5: Stateless Processes

**NEVER** store state in memory between requests.
All persistent data goes to database or S3.

### Rule 6: Leverage Third-Party Libraries First

**YOU MUST** leverage third-party libraries before implementing custom solutions. NEVER reinvent the wheel.

```rust
// ✅ CORRECT — Use established crate
use serde_json::Value;
use chrono::{DateTime, Utc};

// ❌ FORBIDDEN — Custom implementation when library exists
fn parse_json_manually(input: &str) -> Result<MyJson, Error> { ... }
```

Only implement custom solutions if you cannot find a third-party library that properly addresses the problem.

### Rule 7: Assess Library Quality

**YOU MUST** assess third-party library quality before adoption. Check:

- GitHub stars (>500 preferred)
- Last release date (avoid abandoned projects)
- Maintainer activity and community support
- Known CVEs and security issues
- Community adoption and ecosystem fit

### Rule 8: Follow Industry Best Practices

**YOU MUST** follow Rust and general software engineering industry standard best practices:

- SOLID principles
- DRY (Don't Repeat Yourself)
- YAGNI (You Ain't Gonna Need It)
- Rust idioms (ownership, borrowing, error handling)
- Clean code principles
- Security best practices (OWASP guidelines)

### Rule 9: Break Problems into Phases

**YOU MUST** break problems into phases, tasks, and branches:

```bash
# Example breakdown
Phase: Job Management System
├── Task 1: Job entity and database schema    → branch: feature/job-schema
├── Task 2: Job creation API endpoint         → branch: feature/job-create-api
├── Task 3: Job status tracking              → branch: feature/job-status
└── Task 4: Job cancellation logic           → branch: feature/job-cancel
```

- Phase: High-level milestone
- Task: Specific deliverable
- Branch: Git branch per task/fix

### Rule 10: Feature Branch Workflow

**YOU MUST** always work on feature/fix branches. NEVER commit directly to main.

```bash
# ✅ CORRECT — Feature branch workflow
git checkout -b feature/add-webhook-retries
# ... implement, test ...
git add . && git commit -m "feat: add webhook retry logic"
git checkout main
git checkout -b feature/next-task

# ❌ FORBIDDEN — Working on main
git checkout main
# ... make changes directly ...
git commit -m "some changes"
```

**Workflow:**

1. Create feature/fix branch
2. Implement/fix
3. Test thoroughly (`just test`, `just check`)
4. Commit with conventional commit message
5. Switch to new branch for next task
6. Merge to main via PR (when ready for deployment)

### Rule 11: Mutation-Test Critical Logic

**YOU MUST** run `just mutants-domain` after adding or modifying business logic
in `crates/domain/` or `crates/common/`. Fix surviving mutants by adding
targeted test assertions — do NOT use `#[mutants::skip]` to silence legitimate gaps.

Use `#[mutants::skip]` ONLY for:

- Display/Debug impls (cosmetic output)
- Trivial From/Default conversions
- Functions where mutation is meaningless (logging, metrics)

### Compliance Check

Before executing ANY command, ask yourself:

- Is there a Just target for this? → Use it
- Am I about to run cargo/npm/pytest directly? → STOP, use Just
- Does the Just target exist? → If not, create it first
- Did I brainstorm tests before writing code? → Tests first, code second
- Am I using .unwrap() in production code? → STOP, use proper error handling
- Am I hardcoding any config? → Use environment variables only
- Am I storing state in memory? → Use database or S3
- Is there a library for this? → Research before implementing custom
- Did I assess library quality? → Check stars, activity, CVEs
- Am I following best practices? → Review SOLID, DRY, YAGNI principles
- Did I break this into phases/tasks? → Plan before implementing
- Am I working on main branch? → STOP, create feature branch
- Did I modify domain/common logic? → Run `just mutants-domain`
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
just test "job"               # Test matching pattern

# E2E tests (Python)
just test-e2e-mocked          # Fast E2E tests (CI-friendly)
just test-e2e-with-email      # E2E with LocalStack email verification
just test-invitation-workflow # Complete invitation flow (Rust + Python)

# Database
just migrate                  # Run pending migrations
just migrate-new <name>       # Create new migration
just migrate-status           # Check migration status
just seed                     # Seed test data

# Code quality
just fmt                      # Format all code
just clippy                   # Run linter
just fix                      # Auto-fix linting issues

# Build & Deploy
just lambda-build             # Build Lambda with cargo-lambda
just lambda-watch             # Hot reload for local Lambda dev
just deploy-local             # Deploy full stack to LocalStack
just deploy-dev               # Deploy to AWS dev
just deploy-prod              # Deploy to AWS production

# Mutation Testing
just mutants                  # Run mutation tests (domain + common)
just mutants-domain           # Run mutation tests (domain only)
just mutants-check            # Re-test only previously missed mutants

# Infrastructure
just infra-init               # Initialize OpenTofu
just infra-validate           # Validate OpenTofu config
just infra-plan dev           # Plan changes for environment
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

**Error Handling:** `thiserror` for library crates, `anyhow` for application code. Never `.unwrap()` in production.

**Repository Pattern:** Database access via trait-based repositories in `db/` crate, injected into handlers.

**State Machines:** Job/Project/Invitation states defined in `domain/src/state.rs` with explicit transitions.

**Mutation Testing:** `cargo-mutants` validates test effectiveness on domain/common crates.
Surviving mutants indicate tests that don't catch injected bugs — fix by adding assertions.
Config in `.cargo/mutants.toml`. Results in `mutants.out/`.

---

## Tech Stack

| Layer | Technology | 12-Factor Role |
|-------|------------|----------------|
| API | Rust + Lambda | Processes (VI) |
| Database | Supabase | Backing Service (IV) |
| Auth | Supabase Auth | Backing Service (IV) |
| Orchestration | Inngest | Backing Service (IV) |
| Video | RunPod + ComfyUI | Backing Service (IV) |
| AI/LLM | Anthropic Claude | Backing Service (IV) |
| Storage | S3 / LocalStack | Backing Service (IV) |
| IaC | OpenTofu | Build (V) |
| Local Dev | LocalStack + Docker | Dev/Prod Parity (X) |

---

## Project Structure

```
framecast/
├── Cargo.toml              # Dependencies (II)
├── Cargo.lock              # Lockfile (II) ✓ committed
├── Justfile                # Task runner
├── CLAUDE.md               # This file
├── .env.example            # Config template (III)
├── .claude/skills/         # Domain knowledge
├── crates/
│   ├── api/                # Lambda handlers (VI, VII)
│   ├── domain/             # Business logic
│   ├── db/                 # Database layer (IV)
│   ├── email/              # AWS SES email service (IV)
│   ├── inngest/            # Job orchestration (IV)
│   ├── comfyui/            # RunPod client (IV)
│   └── common/             # Shared utilities
├── tests/
│   ├── integration/        # Rust integration tests
│   └── e2e/                # Python E2E tests
│       ├── pyproject.toml  # Dependencies (II)
│       └── uv.lock         # Lockfile (II) ✓ committed
├── infra/
│   ├── opentofu/           # IaC (V)
│   └── runpod/             # Docker images (V, X)
├── migrations/             # Database migrations (XII)
└── scripts/                # Admin processes (XII)
```

---

## Reference Experimental Projects

**Location:** Ubuntu host via SSH: `~/workspace/splice-experimental-1`, `~/workspace/splice-experimental-2`

Consult these for proven patterns when facing implementation challenges
(Rust + Lambda, database schemas, testing patterns,
ComfyUI/RunPod integration, Inngest job orchestration).

---

## Specification Reference

The `docs/spec/` directory contains the formal API specification (v0.0.1-SNAPSHOT):

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

## Skills

The `.claude/skills/` directory contains domain expertise modules:

| Skill | When to Use |
|-------|-------------|
| `api-spec` | API operations, permissions, validation |
| `rust-patterns` | Error handling, repository pattern, handlers |
| `python-e2e` | pytest fixtures, async patterns, mocking |
| `runpod-infra` | Docker images, GPU workload, ComfyUI |
| `observability` | Structured logging, metrics, tracing |
