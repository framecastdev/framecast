# Framecast Project Infrastructure Setup Plan

## Current Status

- ✅ Complete formal specification in `spec/` directory
- ✅ Comprehensive development guidelines (CLAUDE.md with 10 Critical Rules)
- ❌ Core implementation infrastructure missing (greenfield setup required)

## Objective

Set up the complete project infrastructure to enable development according to
the 12-factor methodology and Critical Rules.

## Reference Resources Available

**🚨 CRITICAL:** Reference experimental projects with proven solutions
available via SSH:

- `~/workspace/splice-experimental-1`
- `~/workspace/splice-experimental-2`

These contain battle-tested patterns for:

- Rust + Lambda integration
- Database schemas and migrations
- Testing patterns (unit, integration, E2E)
- ComfyUI/RunPod integration
- Job orchestration with Inngest
- Authentication/authorization
- Infrastructure setup

**Always consult these projects first when facing implementation challenges.**

## Implementation Plan

### Phase 1: Core Build Infrastructure

**Goal:** Establish basic project structure and build system

1. **✅ Create PLAN.md in project root directory**
   - Copy this complete infrastructure setup plan to the project source code
   - Make it version-controlled and accessible to all developers
   - Provide clear roadmap for anyone setting up or contributing to the project

2. **Create Justfile with all referenced commands**
   - Commands: setup, dev, test, test-e2e, check, migrate, build, etc.
   - Ensure compliance with Rule 1 (Just is ONLY entry point)
   - Reference experimental projects for working Just patterns

3. **Create root Cargo.toml workspace configuration**
   - Define workspace structure for all crates
   - Set common dependencies and build settings
   - Follow Rule 2 (Dependencies explicit and isolated)

4. **Create crates/ directory structure**
   - `crates/api/` - Lambda handlers
   - `crates/domain/` - Business logic
   - `crates/db/` - Database layer
   - `crates/inngest/` - Job orchestration
   - `crates/comfyui/` - RunPod client
   - `crates/common/` - Shared utilities

5. **Create .env.example configuration template**
   - All required environment variables
   - Compliance with Rule 4 (Config via Environment Only)

### Phase 2: Testing & Infrastructure

1. **Create tests/e2e/ with Python setup**
   - `pyproject.toml` with pytest, httpx, type hints
   - `conftest.py` with fixtures and mocks
   - Basic test structure following Rule 2 (Tests Before Code)

2. **Create migrations/ directory**
   - Database schema migrations
   - Reference spec/04_Entities.md for entity definitions
   - Ensure all invariants from spec/06_Invariants.md are enforced

3. **Create infra/opentofu/ for IaC**
   - Infrastructure as code setup
   - AWS Lambda, API Gateway, RDS configurations
   - LocalStack for dev/prod parity (Rule 10)

4. **Create scripts/ for admin processes**
   - Database seeding, cleanup, admin tasks
   - Compliance with Rule 12 (Admin processes as code)

### Phase 3: Development Environment

1. **Implement `just setup` command**
    - Install Rust, uv, OpenTofu, LocalStack
    - Verify all dependencies available
    - Create reproducible setup process

2. **Implement `just dev` command**
    - Start LocalStack, Inngest, mock services
    - Ensure dev/prod parity (Rule 10)
    - Port binding configuration (Rule 7)

3. **Implement `just test` command**
    - Run all Rust unit and integration tests
    - Ensure proper error handling (Rule 3: No .unwrap())
    - Test stateless process compliance (Rule 5)

4. **Create first passing test**
    - Validate entire setup works end-to-end
    - Demonstrate all build tools functional
    - Verify environment configuration works

### Phase 4: Validation & Documentation

1. **Run compliance checks against all 10 Critical Rules**
    - Verify Just is only entry point (Rule 1)
    - Confirm tests before code approach (Rule 2)
    - Check no .unwrap() in production (Rule 3)
    - Validate environment-only config (Rule 4)
    - Ensure stateless processes (Rule 5)
    - Verify third-party library usage (Rule 6)
    - Check library quality assessment (Rule 7)
    - Confirm best practices adherence (Rule 8)
    - Validate phase/task breakdown (Rule 9)
    - Check feature branch workflow (Rule 10)

2. **Verify 12-Factor compliance**
    - Single codebase with multiple deploys
    - Explicit dependencies with lockfiles
    - Environment-based configuration
    - Backing services as attached resources
    - Build/release/run separation
    - Stateless processes
    - Port binding
    - Horizontal scaling capability
    - Fast startup/graceful shutdown
    - Dev/prod parity
    - Logs as event streams
    - Admin processes as code

## Critical Files to Reference

- `spec/04_Entities.md` - Database schema requirements
- `spec/06_Invariants.md` - Business rules to enforce
- `spec/07_Operations.md` - API endpoint specifications
- `spec/08_Permissions.md` - Authorization matrix

## Success Criteria

After completion, these commands must work:

```bash
just setup        # Install dependencies ✓
just dev          # Start local environment ✓
just test         # Run tests ✓
just check        # Quality checks ✓
just migrate      # Database migrations ✓
just build        # Build artifacts ✓
```

## Risk Mitigation

- **Technical blockers:** Consult reference experimental projects immediately
- **Pattern uncertainty:** Review working implementations in experimental
  projects
- **Integration issues:** Check proven ComfyUI/RunPod patterns in references
- **Rule violations:** Run compliance check after each phase

## Next Steps After Infrastructure Setup

1. Implement core domain entities (User, Team, Job, etc.)
2. Create API endpoints following spec/07_Operations.md
3. Set up authentication and authorization
4. Implement job processing pipeline
5. Deploy to AWS infrastructure

## Progress Tracking

- [x] Phase 1: Core Build Infrastructure (Steps 1-5)
- [x] Phase 2: Testing & Infrastructure (Steps 6-9)
- [x] Phase 3: Development Environment (Steps 10-13)
- [ ] Phase 4: Validation & Documentation (Steps 14-15)

This plan establishes the foundation for all future development while ensuring
strict adherence to the project's Critical Rules and 12-Factor principles.
