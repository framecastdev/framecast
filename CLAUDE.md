# Framecast API

Storyboard-to-video generation API. Converts YAML/JSON specs → videos via ComfyUI.

## Project Status

**Current Phase:** Specification Complete, Implementation Infrastructure Setup Required

This project has a complete formal specification (see `spec/` directory) and comprehensive development guidelines, but the code infrastructure is not yet established.

### Before You Start:
- [ ] Run project scaffold creation (see Setup Checklist)
- [ ] Verify all build tools are available
- [ ] Set up local environment configuration

### Quick Check:
```bash
# These commands should work after setup:
just setup        # Install dependencies
just dev          # Start local environment
just test         # Run tests
```

If these fail, follow the Setup Checklist below.

## Quick Reference

```bash
just              # Show all commands
just setup        # First-time setup (install all tools)
just dev          # Start local environment
just test         # Run all Rust tests
just test-e2e     # Run E2E tests (mocked)
just check        # Run all quality checks
just migrate      # Run database migrations
just build        # Build release artifacts
```

---

## Setup Checklist

Since this is a greenfield project, you'll need to create the core infrastructure:

### Phase 1: Core Build Infrastructure
- [ ] Create `Justfile` with all referenced commands
- [ ] Create root `Cargo.toml` workspace configuration
- [ ] Create `crates/` directory with individual crates
- [ ] Create `.env.example` configuration template

### Phase 2: Testing & Infrastructure
- [ ] Create `tests/e2e/` with `pyproject.toml` for Python tests
- [ ] Create `migrations/` directory for database schema
- [ ] Create `infra/opentofu/` for infrastructure as code
- [ ] Create `scripts/` for admin tasks

### Phase 3: Development Environment
- [ ] Verify `just setup` installs all dependencies
- [ ] Verify `just dev` starts local services
- [ ] Verify `just test` runs all test suites
- [ ] Create first passing test to validate setup

**Reference:** See the Project Structure section below for expected file organization.

---

## The Twelve-Factor Rules

> **Non-negotiable. These rules align with [12factor.net](https://12factor.net/) for cloud-native apps.**

### I. Codebase — One Repo, Many Deploys

Single monorepo tracked in Git. Same codebase deploys to dev, staging, production.

```
framecast/              # ONE repo
├── crates/             # All Rust code
├── tests/e2e/          # E2E tests
├── infra/              # IaC (OpenTofu)
└── docs/               # Specification
```

- **No** separate repos for frontend/backend/infra
- Branch per feature, merge to main, deploy from main
- Every commit is deployable (CI ensures this)

### II. Dependencies — Explicit and Isolated

All dependencies explicitly declared. Never rely on system-wide packages.

| Language | Declaration | Lockfile |
|----------|-------------|----------|
| Rust | `Cargo.toml` | `Cargo.lock` ✓ committed |
| Python | `pyproject.toml` | `uv.lock` ✓ committed |
| OpenTofu | `versions.tf` | `.terraform.lock.hcl` ✓ committed |

**Dependency Rules:**
- Lockfiles MUST be committed (reproducible builds)
- Prefer popular, well-maintained libraries (>500 GitHub stars)
- Before adopting: check last release date, maintainer activity, CVEs
- `just setup` installs ALL dependencies from scratch

### III. Config — Environment Variables Only

Store config in environment variables. **Never** in code.

```bash
# ✓ CORRECT: Config in environment
DATABASE_URL=${DATABASE_URL}

# ✗ WRONG: Hardcoded config
DATABASE_URL="postgres://localhost:5432/framecast"
```

**Config Categories:**
```bash
# Backing Services (credentials/URLs)
DATABASE_URL=               # Supabase PostgreSQL
SUPABASE_URL=               # Supabase API
SUPABASE_ANON_KEY=          # Public key
SUPABASE_SERVICE_ROLE_KEY=  # Admin key
ANTHROPIC_API_KEY=          # LLM
INNGEST_EVENT_KEY=          # Job orchestration
INNGEST_SIGNING_KEY=
RUNPOD_API_KEY=             # GPU compute
RUNPOD_ENDPOINT_ID=
S3_BUCKET_OUTPUTS=          # Object storage
S3_BUCKET_ASSETS=

# Runtime Config
AWS_REGION=us-east-1
LOG_LEVEL=info
RUST_LOG=framecast=debug
```

**Never commit `.env` files with secrets.** Use `.env.example` as template.

### IV. Backing Services — Attached Resources

Treat all external services as attached resources. Swap without code changes.

| Service | Purpose | Swappable |
|---------|---------|-----------|
| Supabase | Database + Auth | → Any PostgreSQL |
| S3 | Object storage | → LocalStack (dev), R2, MinIO |
| Inngest | Job orchestration | → Via URL config |
| RunPod | GPU compute | → Via endpoint ID |
| Anthropic | LLM | → Via API key |

**Rule:** If connection details change, only environment variables change. Zero code changes.

```rust
// ✓ CORRECT: Resource from config
let db_url = std::env::var("DATABASE_URL")?;

// ✗ WRONG: Hardcoded resource
let db_url = "postgres://localhost/framecast";
```

### V. Build, Release, Run — Strict Separation

Three distinct stages. Never mix them.

```
┌─────────┐     ┌─────────┐     ┌─────────┐
│  BUILD  │ ──► │ RELEASE │ ──► │   RUN   │
│         │     │         │     │         │
│ Code +  │     │ Build + │     │ Execute │
│ Deps    │     │ Config  │     │ Process │
└─────────┘     └─────────┘     └─────────┘
    │               │               │
    │               │               │
  just build    just release     Lambda/
  (artifacts)   (tagged image)   Container
```

**Rules:**
- Build: `just build` creates immutable artifacts (Lambda ZIP, Docker images)
- Release: Artifact + config = deployable unit with unique ID (git SHA)
- Run: Execute in target environment
- **Never** modify code at runtime
- **Never** apply config at build time

### VI. Processes — Stateless and Share-Nothing

Execute as stateless processes. Store persistent data in backing services.

```rust
// ✓ CORRECT: Stateless handler
async fn handle_request(req: Request, db: &Pool) -> Response {
    let data = db.query(&req.id).await?;  // State in DB
    process(data)
}

// ✗ WRONG: In-memory state
static CACHE: Mutex<HashMap<String, Data>> = ...;  // Lost on restart
```

**Rules:**
- Lambda functions are inherently stateless — embrace this
- Session data → Supabase
- File uploads → S3 (never local filesystem)
- Job state → Database (Job entity with status)
- **Sticky sessions are forbidden**

### VII. Port Binding — Self-Contained Services

Export services via port binding. No external web server injection.

```bash
# Local development
just dev              # Starts services on defined ports
  → API:      localhost:3000
  → Inngest:  localhost:8288
  → LocalStack: localhost:4566

# Production
Lambda → API Gateway (AWS handles port binding)
```

**Rule:** The app is self-contained. It includes HTTP server code (axum in Rust).

### VIII. Concurrency — Scale Out via Process Model

Scale horizontally by running more processes, not bigger machines.

```
        ┌──────────┐
        │ API GW   │
        └────┬─────┘
             │
    ┌────────┼────────┐
    │        │        │
┌───▼──┐ ┌───▼──┐ ┌───▼──┐
│Lambda│ │Lambda│ │Lambda│   ← Horizontal scaling
│ (1)  │ │ (2)  │ │ (n)  │
└──────┘ └──────┘ └──────┘
```

**Rules:**
- Lambda scales automatically (concurrency limit in config)
- RunPod workers scale via endpoint replicas
- Database connections pooled (PgBouncer via Supabase)
- **Never** scale by adding RAM/CPU to single process

### IX. Disposability — Fast Startup, Graceful Shutdown

Processes start fast and shut down gracefully.

**Startup:**
- Lambda cold start < 500ms (Rust helps here)
- No heavy initialization in handler path
- Lazy-load expensive resources

**Shutdown:**
- Handle SIGTERM gracefully
- Finish in-flight requests
- Release database connections
- Jobs: checkpoint progress before exit (Inngest handles this)

```rust
// Graceful shutdown pattern
async fn main() {
    let shutdown = signal::ctrl_c();
    
    tokio::select! {
        _ = server.serve() => {},
        _ = shutdown => {
            // Cleanup
            pool.close().await;
        }
    }
}
```

### X. Dev/Prod Parity — Keep Environments Identical

Minimize gaps between development and production.

| Gap | Bad | Good |
|-----|-----|------|
| Time | Weeks between deploys | Hours (CI/CD) |
| Personnel | Devs write, ops deploy | Same person does both |
| Tools | SQLite dev, Postgres prod | Postgres everywhere |

**Our Parity:**
- LocalStack mimics AWS S3 locally
- Same PostgreSQL (Supabase) in dev and prod
- Same Inngest for job orchestration
- Docker ensures RunPod workers identical
- `just dev` starts production-equivalent stack

**Rules:**
- **Never** use SQLite for dev if prod is Postgres
- **Never** mock S3 with filesystem
- `just dev` == prod (minus scale)

### XI. Logs — Event Streams to stdout

Treat logs as event streams. Never manage log files.

```rust
// ✓ CORRECT: Structured JSON to stdout
tracing::info!(
    job_id = %job.id,
    status = %job.status,
    "Job status changed"
);

// ✗ WRONG: Writing to files
let mut file = File::create("/var/log/app.log")?;
```

**Rules:**
- All logs to stdout/stderr (Lambda captures automatically)
- Structured JSON format (machine-parseable)
- Include correlation IDs (request_id, job_id)
- Log aggregation is infra concern (CloudWatch, Datadog)
- **Never** `println!()` for logging — use `tracing`

### XII. Admin Processes — One-Off Tasks as Code

Run admin tasks as one-off processes in identical environment.

```bash
# ✓ CORRECT: Admin tasks via Just
just migrate              # Database migrations
just seed                 # Seed test data
just cleanup-jobs         # Archive old jobs
just generate-api-key     # Create admin key

# ✗ WRONG: Manual SQL in production
psql -c "UPDATE users SET tier='creator'..."
```

**Rules:**
- Admin scripts live in repo (same codebase)
- Run with same config as app processes
- Migrations are versioned and reversible
- One-off tasks have Just targets
- **Never** run ad-hoc SQL in production

---

## Project-Specific Rules

### P1: Just is the Frontend

Every task has a Just target. If it doesn't exist, create it first.

```bash
just <task>     # Always the entry point
```

### P2: Tests Before Code

Brainstorm test cases BEFORE implementation. Cover:
- Happy path
- Edge cases (boundaries, empty, max)
- Error conditions
- Invariant violations

### P3: E2E Tests in Python

```bash
tests/e2e/
├── conftest.py         # Fixtures, mocks
├── test_jobs.py        # Job lifecycle
├── test_teams.py       # Team operations
└── ...
```

- pytest + httpx + type hints
- Two modes: mocked (CI) and real RunPod (pre-release)
- uv for package management

### P4: Async Event-Based

Prefer async events over polling.

```
Job Created → Inngest Event → Worker Processes → Webhook Delivery
           (not polling)              (not polling)
```

### P5: Follow Rust Idioms

- Error handling: `thiserror` for libraries, `anyhow` for apps
- No `.unwrap()` in production code
- Use `?` operator, not `match` for simple error propagation
- Structured errors with context

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

## Local Development Quick Start

### First Time Setup
```bash
# 1. Install required tools
just setup                    # Installs Rust, uv, OpenTofu, LocalStack

# 2. Configure environment
cp .env.example .env         # Edit with your credentials
source .env

# 3. Start local services
just dev                     # LocalStack, Inngest, mock services
```

### Development Workflow
```bash
# Start development session
just dev                     # Terminal 1: Services
just test --watch           # Terminal 2: Auto-test
just logs                   # Terminal 3: Log aggregation

# Make changes, then:
just test                   # Run full test suite
just check                  # Linting, format, invariants
git add . && git commit -m "feat: your change"
```

### Common Commands
- `just test-e2e-mocked` - Fast E2E tests (CI-friendly)
- `just test-e2e-real` - Full E2E tests with real RunPod
- `just migrate` - Apply database migrations
- `just seed` - Load test data

---

## Specification Integration

The `spec/` directory contains the formal API specification (v0.4.3). Key files for implementation:

### Core Reference Files
- `spec/00_Index.md` - Specification overview and changelog
- `spec/04_Entities.md` - Database entities, field definitions
- `spec/05_Relationships_States.md` - State machines, entity relationships
- `spec/06_Invariants.md` - Business rules that MUST be enforced
- `spec/07_Operations.md` - API endpoint specifications
- `spec/08_Permissions.md` - Role-based access control matrix

### Implementation Guidance
- Read `04_Entities.md` before creating any database schema
- Check `06_Invariants.md` for all validation rules
- Reference `07_Operations.md` for endpoint requirements
- Use `08_Permissions.md` for authorization logic

### Spec Versioning
Current version: v0.4.3 (2025-01-30)
- Breaking changes require version bump
- Implementation must match spec version exactly

---

## Domain Model

**User Tiers**: Visitor → Starter → Creator

**URN Ownership**:
- `framecast:user:usr_X` - Personal (Starter or Creator)
- `framecast:team:tm_X` - Team-shared (Creator only)
- `framecast:tm_X:usr_Y` - User's work within team (Creator only)

**Core Entities**: User, Team, Membership, Project, Job, AssetFile, Webhook, ApiKey

**Job States**: queued → processing → completed/failed/canceled

---

## Key Invariants

1. Every team has ≥1 owner (INV-T2)
2. Only creators can have team memberships (INV-M4)
3. Starters have no team memberships (INV-U3)
4. Project jobs must be team-owned (INV-J11)
5. Max 1 active job per project (INV-J12)
6. Credits cannot go negative (INV-U5, INV-T6)
7. Refunds ≤ charges (INV-J8)
8. Max 10 owned teams per user (CARD-2)
9. Max 5 concurrent jobs per team (CARD-5)
10. Max 1 concurrent job per starter (CARD-6)

---

## Git Workflow

```bash
# Branch naming (I. Codebase)
feature/add-webhook-retries
fix/job-cancel-refund
refactor/extract-urn-parser

# Commits: conventional commits
feat: add webhook retry logic
fix: correct refund calculation for canceled jobs
refactor: extract URN parser to common crate
test: add E2E tests for team invitation flow
docs: update API spec for v0.4.3
```

---

## When Implementing Features

### Prerequisites
1. Ensure all setup checklist items are completed
2. Read relevant spec files (located in `spec/` directory)
3. Load appropriate Claude skills (see Skills section)

### Implementation Workflow
1. **Plan & Research**
   - Read spec files: `04_Entities.md`, `06_Invariants.md`, `07_Operations.md`
   - Review similar implementations in existing crates
   - Brainstorm test cases FIRST (P2: Tests Before Code)

2. **Create Tests**
   - Unit tests in the crate's `src/` directory
   - Integration tests in `tests/integration/`
   - E2E tests in `tests/e2e/` (Python)

3. **Implement**
   - Follow Rust patterns from `.claude/skills/rust-patterns/`
   - Keep processes stateless (VI)
   - Use environment variables for config (III)

4. **Validate**
   - Run `just check` (linting, tests, invariants)
   - Verify observability (logging, metrics)
   - Test configuration via environment variables

5. **Document & Commit**
   - Update relevant spec files if needed
   - Use conventional commit messages
   - Ensure no hardcoded config remains

---

## Skills Reference

The `.claude/skills/` directory contains domain expertise modules. Use the Skill tool in Claude Code to load these:

| Skill | When to Use | Key Capabilities |
|-------|-------------|------------------|
| `api-spec` | API operations, validation | Reference spec files, check permissions, validate requests |
| `rust-patterns` | Writing Rust code | Error handling, repository pattern, handler structure |
| `python-e2e` | E2E test development | pytest fixtures, async patterns, RunPod mocking |
| `runpod-infra` | Infrastructure work | Docker images, model downloads, GPU workload management |
| `observability` | Logging, debugging | Structured logging, metrics, health checks |

### Usage Example:
When implementing a new API endpoint:
1. Use `api-spec` skill to understand operation requirements
2. Use `rust-patterns` skill for handler implementation
3. Use `python-e2e` skill to create comprehensive tests
4. Use `observability` skill to add proper logging

---

## Spec Reference

Key files (attached to project):
- `04_Entities.md` - Entity definitions
- `05_Relationships_States.md` - State machines
- `06_Invariants.md` - Business rules
- `07_Operations.md` - API operations
- `08_Permissions.md` - Permission matrix
- `09_Validation.md` - Webhook payloads
- `11_Storage.md` - Credit policies
