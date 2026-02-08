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
3. Commit with conventional commit message
4. Push and let CI verify
5. Switch to new branch for next task
6. Merge to main via PR (when ready for deployment)

### Rule 11: CI Runs All Checks — Never Run Checks Locally

**NEVER** run tests, clippy, fmt, or any verification commands locally.
CI is the single source of truth for all checks. After committing and pushing,
let CI validate the change.

```bash
# ❌ FORBIDDEN — Never run checks locally
just test            # WRONG
just check           # WRONG
just clippy          # WRONG
just fmt             # WRONG
just ci-clippy       # WRONG
just mutants-domain  # WRONG

# ✅ CORRECT — Commit, push, let CI run
git add <files> && git commit -m "feat: ..."
git push
# Then wait for CI results
```

### Rule 12: No Placeholder Code

**NEVER** add code "for future use." This includes:

- Function parameters not used by the current implementation
- Empty function bodies, if-blocks, or match arms with only comments
- Stub files containing only comments ("will be expanded in the next phase")
- Feature-flag-style dead code paths

If logic is deferred to another layer (e.g., repository), do NOT scaffold it
in the current layer. Add it when it's actually implemented.

YAGNI: code that does nothing today is noise — it creates unused API surface,
confuses mutation testing, and misleads readers about what the code actually does.

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
- Am I about to run tests/clippy/fmt/checks locally? → STOP, let CI do it
- Am I adding placeholder code? → STOP, YAGNI — add it when it's needed
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
just test teams               # Test specific crate
just test "job"               # Test matching pattern

# E2E tests (Python)
just test-e2e                  # Run all E2E tests (requires local services)

# Database
just migrate                  # Run pending migrations
just migrate-new <name>       # Create new migration
just migrate-status           # Check migration status

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
just mutants                  # Run mutation tests (all domain crates + common)
just mutants-domain           # Run mutation tests (domain crates only)
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
domains/                   # Domain-driven vertical slices
├── teams/                 # framecast-teams: Users, Teams, Memberships, Invitations, ApiKeys, Auth
├── projects/              # framecast-projects: Projects, AssetFiles (stub)
├── jobs/                  # framecast-jobs: Jobs, JobEvents (stub)
└── webhooks/              # framecast-webhooks: Webhooks, WebhookDeliveries (stub)

crates/                    # Shared infrastructure
├── app/                   # framecast-app: Composition root, Lambda + local binaries
├── email/                 # framecast-email: AWS SES email service
└── common/                # framecast-common: Shared error types, URN parsing
```

Each domain crate owns its full vertical slice:

```
domains/teams/src/
├── api/                   # Routes, handlers, middleware (axum)
│   ├── middleware.rs       # AuthUser, ApiKeyUser, TeamsState, AuthConfig
│   ├── routes.rs           # Router<TeamsState>
│   └── handlers/           # users.rs, teams.rs, memberships.rs
├── domain/                # Entities, state machines, validation
│   ├── entities.rs         # User, Team, Membership, Invitation, ApiKey
│   ├── state.rs            # InvitationStateMachine
│   ├── auth.rs             # AuthContext
│   └── validation.rs       # validate_team_slug
└── repository/            # Database access (sqlx + PostgreSQL)
    ├── users.rs, teams.rs, memberships.rs, invitations.rs, api_keys.rs
    └── transactions.rs     # TX helpers
```

### Dependency Flow

```
                  framecast-common
                   ↑    ↑    ↑    ↑
            ┌──────┘    │    │    └──────┐
    framecast-teams     │    │     framecast-email
         ↑              │    │
         │    ┌─────────┘    │
    framecast-jobs           │
         ↑                   │
    framecast-webhooks ──────┘
         ↑
    framecast-app → ALL domains + email
```

- Each domain owns entities + repositories + API handlers (vertical slice)
- `framecast-teams` has no domain dependencies (only `common` + `email`)
- `framecast-app` is the composition root (composes all domain routers)
- Cross-domain reads use CQRS: query other domain's tables directly (same DB)

### Key Patterns

**Error Handling:** `thiserror` for library crates, `anyhow` for application code. Never `.unwrap()` in production.

**Domain State:** Each domain defines its own state (e.g. `TeamsState { repos, auth_config, email }`).
The app crate composes them via `Router::merge()` with `.with_state()`.

**Repository Pattern:** Per-domain repository structs (e.g. `TeamsRepositories`) with per-entity repos.
Cross-domain queries read tables directly (CQRS read-side).

**State Machines:** Job/Project/Invitation states defined in each domain's `domain/state.rs` with explicit transitions.

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
├── domains/
│   ├── teams/              # Users, Teams, Memberships, Invitations, Auth
│   ├── projects/           # Projects, AssetFiles (stub)
│   ├── jobs/               # Jobs, JobEvents (stub)
│   └── webhooks/           # Webhooks, WebhookDeliveries (stub)
├── crates/
│   ├── app/                # Composition root, Lambda + local binaries
│   ├── email/              # AWS SES email service (IV)
│   └── common/             # Shared utilities
├── tests/
│   ├── integration/        # Rust integration tests
│   └── e2e/                # Python E2E tests
│       ├── pyproject.toml  # Dependencies (II)
│       └── uv.lock         # Lockfile (II) ✓ committed
├── infra/
│   └── opentofu/           # IaC (V)
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
| `python-e2e` | pytest fixtures, async patterns |
| `runpod-infra` | Docker images, GPU workload, ComfyUI |
| `observability` | Structured logging, metrics, tracing |
