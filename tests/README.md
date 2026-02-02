# Framecast Test Suite - Rule 2 Compliance

**Following Rule 2: Tests Before Code**

This comprehensive test suite validates all components with brainstormed test cases covering happy paths, edge cases, error conditions, and invariants.

## Test Strategy Overview

All test cases were brainstormed BEFORE implementation in `TEST_STRATEGY.md`, following Rule 2:

- **Happy Path Tests**: Normal operation scenarios
- **Edge Cases**: Boundary conditions and unusual inputs
- **Error Conditions**: Failure scenarios and error handling
- **Invariant Tests**: Business rule enforcement
- **Performance Tests**: Timing and resource usage validation
- **Security Tests**: Authentication, authorization, and data protection

## Test Categories

### 1. Database Migration Tests (`tests/migrations/`)

Validates database schema creation, business rules, and data integrity.

**Key Test Cases:**

- `HAPPY-01`: Clean migration from scratch
- `HAPPY-02`: Migration status tracking
- `HAPPY-03`: Business logic triggers
- `EDGE-01`: Migration idempotency
- `ERROR-01`: Connection failure handling
- `INV-01-04`: All business rule constraints (user credits, team ownership, job limits, URN validation)

### 2. Admin Script Tests (`tests/admin_scripts/`)

Validates operational scripts for seeding, cleanup, export, and monitoring.

**Key Test Cases:**

- **Seeding**: Complete test data creation, clear/re-seed, existing data handling
- **Cleanup**: Job retention policies, S3 object removal, dry-run mode
- **Export**: GDPR compliance, large datasets, user identification
- **API Keys**: URN validation, ownership rules, security
- **Health**: Service monitoring, mixed states, failure detection

### 3. Integration Tests

End-to-end workflows validating component interactions.

### 4. Performance Tests

Resource usage, timing, and scalability validation.

### 5. Security Tests

Authentication, authorization, and data protection validation.

## Running Tests

### Quick Test Execution

```bash
# Run all tests
python tests/run_tests.py --all

# Run specific category
python tests/run_tests.py --migrations
python tests/run_tests.py --admin-scripts
python tests/run_tests.py --unit

# Performance tests
python tests/run_tests.py --performance

# Coverage report
python tests/run_tests.py --coverage
```

### Using Just Commands

```bash
# Install test dependencies
just test-install-deps

# Run comprehensive test suite
just test-comprehensive

# Run specific test categories
just test-migrations
just test-admin-scripts
just test-integration
```

### Manual pytest Execution

```bash
# Install test dependencies
pip install -r tests/requirements.txt

# Run migration tests
pytest tests/migrations/ -v --tb=short

# Run admin script tests
pytest tests/admin_scripts/ -v --tb=short

# Run with markers
pytest tests/ -m "not slow" -v          # Skip slow tests
pytest tests/ -m "integration" -v       # Integration tests only
pytest tests/ -m "performance" -v       # Performance tests only
```

## Test Configuration

### Environment Variables

Tests use isolated test databases and mock services:

```bash
# Test database (isolated from development data)
TEST_DATABASE_URL=postgresql://postgres:password@localhost:5432/framecast_test

# Mock service endpoints
LOCALSTACK_ENDPOINT=http://localhost:4566
INNGEST_ENDPOINT=http://localhost:8288

# Test S3 buckets
S3_BUCKET_OUTPUTS=test-framecast-outputs
S3_BUCKET_ASSETS=test-framecast-assets
```

### Test Database Management

- Each test gets isolated database created/destroyed automatically
- Migration tests start with clean database and run actual migrations
- Admin script tests include full migration setup
- Automatic cleanup prevents test database accumulation

## Test Implementation Details

### Database Migration Testing

- **Isolated Databases**: Each test creates temporary database
- **Real Migrations**: Uses actual sqlx migration files
- **Constraint Validation**: Tests business rules enforcement
- **Performance Measurement**: Migration timing validation

### Admin Script Testing

- **Mocked Dependencies**: S3, external APIs mocked where appropriate
- **Real Database Operations**: Uses actual database for integration testing
- **Safety Testing**: Validates dry-run modes and confirmation prompts
- **Error Simulation**: Tests network failures, permission errors

### Test Data Management

- **Fixtures**: Reusable test data setup/teardown
- **Factories**: Dynamic test data generation
- **Isolation**: No test interdependencies
- **Cleanup**: Automatic resource cleanup after tests

## Test Markers

Tests use pytest markers for categorization:

- `@pytest.mark.slow`: Long-running tests (skip with `-m "not slow"`)
- `@pytest.mark.integration`: Cross-component tests
- `@pytest.mark.performance`: Performance/load tests
- `@pytest.mark.requires_database`: Database connection required

## Coverage Requirements

Target coverage levels:

- **Database Migrations**: 100% (all SQL statements tested)
- **Admin Scripts**: 90%+ (all major functions and error paths)
- **Integration Workflows**: 80%+ (all user-facing scenarios)
- **Business Rules**: 100% (all invariants validated)

## Adding New Tests

When adding new components, follow Rule 2:

1. **Brainstorm Test Cases** in `TEST_STRATEGY.md`:
   - Happy path scenarios
   - Edge cases and boundaries
   - Error conditions
   - Business rule validation

2. **Create Test Files**:
   - Use descriptive test names matching strategy
   - Include setup/teardown fixtures
   - Add appropriate pytest markers

3. **Implement Tests**:
   - Follow existing patterns
   - Use proper assertions
   - Include error message validation

4. **Update Documentation**:
   - Add to this README
   - Update test runner if needed
   - Document any new dependencies

## Continuous Integration

Tests integrate with CI/CD pipeline:

- **Pre-commit**: Fast test subset
- **Pull Request**: Full test suite
- **Deploy**: Performance regression testing
- **Nightly**: Security scans and load testing

## Troubleshooting

### Common Issues

**Database Connection Errors:**

```bash
# Ensure PostgreSQL running
docker compose -f docker-compose.local.yml up -d postgres

# Check connection
psql postgresql://postgres:dev-password-framecast@localhost:5432/postgres
```

**Missing Dependencies:**

```bash
# Install test requirements
pip install -r tests/requirements.txt

# Or use test runner
python tests/run_tests.py --install-deps
```

**Slow Tests:**

```bash
# Skip slow tests during development
pytest tests/ -m "not slow" -v

# Run slow tests separately
pytest tests/ -m "slow" -v
```

**Memory Issues with Large Tests:**

```bash
# Run tests with memory profiling
pytest tests/ --profile-mem

# Limit concurrent tests
pytest tests/ -n 1
```

This test suite ensures comprehensive validation while following Rule 2: Tests Before Code, providing confidence in all system components.
