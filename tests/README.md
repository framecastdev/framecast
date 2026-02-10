# Framecast Test Suite

## Test Categories

### Rust Unit Tests (domain logic)

Located in each domain crate's source files (`#[cfg(test)]` modules).
Covers entities, state machines, validation logic.

### Rust Integration Tests

Located in `tests/integration/`. Tests API handlers with real database connections.

### Python E2E Tests

Located in `tests/e2e/`. Full end-to-end tests against a running API server.

## Running Tests

```bash
# Run all Rust tests (unit + integration)
just test

# Run tests for a specific crate
just test teams

# Run tests matching a pattern
just test "invitation"

# Run E2E tests (requires local services via `just dev`)
just test-e2e
```

### CI-specific recipes

```bash
# CI unit tests (excludes integration tests)
just ci-test

# CI integration tests (requires DB)
just ci-test-integration

# CI E2E tests (requires running API + services)
just ci-test-e2e

# Mutation testing
just ci-mutants
```

## Mutation Testing

Uses `cargo-mutants` to validate test effectiveness. Mutants inject small
changes into production code; surviving mutants indicate missing test assertions.

```bash
# Run mutation tests on all domain + common crates
just mutants

# Run mutation tests on domain crates only
just mutants-domain

# Re-test only previously missed mutants
just mutants-check
```

Results are saved to `mutants.out/`. Config in `.cargo/mutants.toml`.

## Environment Variables

Tests use isolated databases and mock services:

```bash
TEST_DATABASE_URL=postgresql://postgres:password@localhost:5432/framecast_test  # pragma: allowlist secret
TEST_JWT_SECRET=test-secret  # pragma: allowlist secret
TEST_API_BASE_URL=http://localhost:3000
```

## Adding New Tests

Follow Rule 2 (Tests Before Code):

1. Brainstorm test cases covering: happy path, edge cases, error conditions, invariants
2. Write tests in the appropriate location (unit in domain crate, integration in `tests/integration/`, E2E in `tests/e2e/`)
3. Implement the feature
4. Push and let CI verify
