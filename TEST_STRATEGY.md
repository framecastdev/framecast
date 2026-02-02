# Framecast Test Strategy - Rule 2 Compliance

**Following Rule 2: Tests Before Code**
This document brainstorms test cases BEFORE implementing tests, covering happy path, edge cases, error conditions, and invariants.

---

## 1. DATABASE MIGRATIONS TEST CASES

### Happy Path Tests

- **HAPPY-01**: Clean database migration from scratch
  - Given: Empty PostgreSQL database
  - When: Run migrations in sequence
  - Then: All tables, constraints, indexes created successfully
  - Verify: All 14 tables exist with correct schema

- **HAPPY-02**: Migration status tracking works correctly
  - Given: Migrations applied
  - When: Check migration status
  - Then: Returns correct applied/pending status for each migration

- **HAPPY-03**: Business logic triggers function correctly
  - Given: Migration applied with triggers
  - When: Insert data that should trigger business logic
  - Then: Triggers fire and enforce rules correctly

### Edge Cases

- **EDGE-01**: Migration on database with existing data
  - Given: Database with some existing tables/data
  - When: Run migrations
  - Then: Migrations handle existing structures gracefully

- **EDGE-02**: Partial migration failure recovery
  - Given: Migration that fails halfway through
  - When: Fix issue and retry migration
  - Then: Can recover and complete successfully

- **EDGE-03**: Large batch constraint validation
  - Given: Migration adds new constraints
  - When: Existing data violates new constraints
  - Then: Migration fails safely with clear error message

- **EDGE-04**: Concurrent migration attempts
  - Given: Migration running in one session
  - When: Another session tries to run migrations
  - Then: Second session waits or fails safely

### Error Conditions

- **ERROR-01**: Invalid SQL syntax in migration
  - Given: Migration with syntax error
  - When: Attempt to run migration
  - Then: Migration fails with clear error, no partial application

- **ERROR-02**: Database connection lost during migration
  - Given: Migration in progress
  - When: Database connection drops
  - Then: Transaction rolls back, database remains consistent

- **ERROR-03**: Insufficient database permissions
  - Given: User without CREATE/ALTER permissions
  - When: Run migrations
  - Then: Clear permission error, no partial changes

- **ERROR-04**: Constraint violation during migration
  - Given: Migration that violates existing business rules
  - When: Run migration
  - Then: Migration fails with constraint details

### Invariant Tests

- **INV-01**: User credit constraints always enforced
  - Test: User.credits ≥ 0 cannot be violated
  - Test: Team.credits ≥ 0 cannot be violated

- **INV-02**: Team ownership rules enforced
  - Test: Cannot delete last owner from team
  - Test: Every team has ≥1 member at all times

- **INV-03**: Job concurrency limits enforced
  - Test: Starter users cannot exceed 1 concurrent job
  - Test: Teams cannot exceed 5 concurrent jobs
  - Test: Projects cannot have >1 active job

- **INV-04**: URN validation working
  - Test: Invalid URN formats rejected
  - Test: Starter users cannot have team URNs
  - Test: API key ownership validated properly

---

## 2. INFRASTRUCTURE (OpenTofu) TEST CASES

### Happy Path Tests

- **INFRA-HAPPY-01**: Complete infrastructure deployment
  - Given: Clean AWS account/region
  - When: Deploy with valid configuration
  - Then: All resources created with correct relationships

- **INFRA-HAPPY-02**: Multi-environment deployment
  - Given: Dev/staging/prod configurations
  - When: Deploy each environment
  - Then: Resources properly isolated and configured per environment

- **INFRA-HAPPY-03**: Lambda function deployment
  - Given: Built Lambda package
  - When: Deploy infrastructure
  - Then: Lambda function created and accessible via API Gateway

### Edge Cases

- **INFRA-EDGE-01**: Resource name conflicts
  - Given: Infrastructure with existing resource names
  - When: Deploy to same region
  - Then: Handles naming conflicts gracefully

- **INFRA-EDGE-02**: Partial resource creation failure
  - Given: AWS service limit reached during deployment
  - When: Continue deployment
  - Then: Fails safely, allows cleanup/retry

- **INFRA-EDGE-03**: Cross-region deployment
  - Given: Different AWS regions specified
  - When: Deploy infrastructure
  - Then: Region-specific resources created correctly

### Error Conditions

- **INFRA-ERROR-01**: Invalid AWS credentials
  - Given: Expired/invalid credentials
  - When: Attempt deployment
  - Then: Clear authentication error, no partial resources

- **INFRA-ERROR-02**: Missing required variables
  - Given: Configuration missing critical variables
  - When: Plan/apply infrastructure
  - Then: Validation fails before any resource creation

- **INFRA-ERROR-03**: Service quota exceeded
  - Given: AWS quota limits reached
  - When: Deploy infrastructure
  - Then: Clear quota error, no orphaned resources

- **INFRA-ERROR-04**: Invalid Terraform syntax
  - Given: .tf files with syntax errors
  - When: Run terraform plan
  - Then: Syntax validation fails before execution

### Invariant Tests

- **INFRA-INV-01**: Security best practices enforced
  - Test: S3 buckets have public access blocked
  - Test: IAM roles follow least-privilege principle
  - Test: Security groups only allow required ports

- **INFRA-INV-02**: Cost optimization verified
  - Test: Lifecycle policies configured on S3
  - Test: Appropriate instance sizes per environment
  - Test: Log retention policies configured

- **INFRA-INV-03**: High availability patterns
  - Test: Resources deployed across availability zones
  - Test: Backup and monitoring configured for production
  - Test: Auto-scaling configured where appropriate

---

## 3. ADMIN SCRIPTS TEST CASES

### A. Database Seeding Script

#### Happy Path Tests

- **SEED-HAPPY-01**: Complete test data creation
  - Given: Clean database with migrations applied
  - When: Run seeding script
  - Then: Creates complete test dataset with valid relationships

- **SEED-HAPPY-02**: Clear and re-seed functionality
  - Given: Database with existing test data
  - When: Run seed script with --clear flag
  - Then: Removes old test data and creates fresh dataset

#### Edge Cases

- **SEED-EDGE-01**: Seed with existing production data
  - Given: Database with real production data
  - When: Run seeding script
  - Then: Only creates test data, doesn't affect production data

- **SEED-EDGE-02**: Partial seeding failure recovery
  - Given: Seeding fails halfway through
  - When: Re-run seeding script
  - Then: Can detect partial state and complete successfully

#### Error Conditions

- **SEED-ERROR-01**: Database constraint violations
  - Given: Seeding script tries to violate business rules
  - When: Script execution
  - Then: Fails with clear constraint violation message

- **SEED-ERROR-02**: Database connectivity issues
  - Given: Database unavailable during seeding
  - When: Script runs
  - Then: Fails gracefully with connection error

#### Invariant Tests

- **SEED-INV-01**: All seeded data follows business rules
  - Test: Created users/teams follow tier restrictions
  - Test: Job statuses are valid
  - Test: URN ownership is correct

### B. Job Cleanup Script

#### Happy Path Tests

- **CLEANUP-HAPPY-01**: Old job cleanup with S3 objects
  - Given: Database with old completed jobs and S3 objects
  - When: Run cleanup script with appropriate retention
  - Then: Removes old jobs and associated S3 objects

- **CLEANUP-HAPPY-02**: Dry-run mode functionality
  - Given: Old jobs in database
  - When: Run cleanup with --dry-run
  - Then: Reports what would be deleted without making changes

#### Edge Cases

- **CLEANUP-EDGE-01**: Large dataset cleanup
  - Given: Thousands of old jobs to clean up
  - When: Run cleanup script
  - Then: Processes in batches without memory issues

- **CLEANUP-EDGE-02**: Mixed job statuses cleanup
  - Given: Jobs in various terminal states
  - When: Run cleanup script
  - Then: Only removes appropriate jobs based on status and age

#### Error Conditions

- **CLEANUP-ERROR-01**: S3 permission errors during cleanup
  - Given: Insufficient S3 permissions
  - When: Attempt to delete objects
  - Then: Continues with database cleanup, reports S3 errors

- **CLEANUP-ERROR-02**: Database constraint violations
  - Given: Jobs with foreign key dependencies
  - When: Attempt cleanup
  - Then: Handles cascading deletes correctly or reports conflicts

### C. User Data Export Script

#### Happy Path Tests

- **EXPORT-HAPPY-01**: Complete user data export
  - Given: User with full activity history
  - When: Export user data
  - Then: Creates complete JSON export with all related data

- **EXPORT-HAPPY-02**: Export by email or user ID
  - Given: Valid user identifier (email or UUID)
  - When: Run export script
  - Then: Successfully identifies user and exports data

#### Edge Cases

- **EXPORT-EDGE-01**: User with no activity
  - Given: Newly created user with minimal data
  - When: Export user data
  - Then: Creates valid export with empty collections where appropriate

- **EXPORT-EDGE-02**: User with large dataset
  - Given: User with extensive history (thousands of jobs)
  - When: Export data
  - Then: Handles large datasets without memory issues

#### Error Conditions

- **EXPORT-ERROR-01**: Non-existent user
  - Given: Invalid user identifier
  - When: Attempt export
  - Then: Clear error message, no export file created

- **EXPORT-ERROR-02**: File system permissions
  - Given: No write permission to output directory
  - When: Attempt export
  - Then: Clear permission error before data processing

### D. API Key Generation Script

#### Happy Path Tests

- **APIKEY-HAPPY-01**: Generate personal API key
  - Given: Valid user email
  - When: Generate API key for user URN
  - Then: Creates valid API key with proper hash and ownership

- **APIKEY-HAPPY-02**: Generate team API key
  - Given: Creator user with team membership
  - When: Generate API key for team URN
  - Then: Creates team API key with proper ownership validation

#### Edge Cases

- **APIKEY-EDGE-01**: Multiple API keys per user
  - Given: User with existing API keys
  - When: Generate additional API key
  - Then: Creates new key without affecting existing ones

- **APIKEY-EDGE-02**: API key with expiration
  - Given: Request for temporary API key
  - When: Generate with expiration
  - Then: Creates key with correct expiration date

#### Error Conditions

- **APIKEY-ERROR-01**: Invalid URN ownership
  - Given: Starter user requesting team URN
  - When: Attempt to generate API key
  - Then: Validates ownership and rejects with clear error

- **APIKEY-ERROR-02**: Non-existent user
  - Given: Invalid user email
  - When: Attempt API key generation
  - Then: Clear user not found error

### E. Health Check Script

#### Happy Path Tests

- **HEALTH-HAPPY-01**: All services healthy
  - Given: All backing services running correctly
  - When: Run health check
  - Then: Reports all services as healthy with detailed status

- **HEALTH-HAPPY-02**: JSON output format
  - Given: Health check with --json flag
  - When: Run health check
  - Then: Outputs structured JSON with all service details

#### Edge Cases

- **HEALTH-EDGE-01**: Mixed service states
  - Given: Some services healthy, others degraded
  - When: Run health check
  - Then: Reports accurate individual and overall status

- **HEALTH-EDGE-02**: Service recovery detection
  - Given: Previously failed service now healthy
  - When: Run health check
  - Then: Detects recovery and reports current healthy state

#### Error Conditions

- **HEALTH-ERROR-01**: All services down
  - Given: No backing services available
  - When: Run health check
  - Then: Reports all failures with appropriate exit codes

- **HEALTH-ERROR-02**: Network connectivity issues
  - Given: Network preventing service connections
  - When: Run health check
  - Then: Distinguishes network vs service issues

---

## 4. INTEGRATION TEST SCENARIOS

### End-to-End Workflow Tests

- **E2E-01**: Complete development setup
  - Setup fresh environment → Run migrations → Seed data → Health check
  - All steps complete successfully with proper data relationships

- **E2E-02**: Admin workflow simulation
  - Create API key → Export user data → Clean old jobs → Verify health
  - All admin tasks complete without data corruption

- **E2E-03**: Infrastructure deployment workflow
  - Plan infrastructure → Deploy → Verify resources → Update configuration
  - Complete infrastructure lifecycle without orphaned resources

### Cross-Component Integration

- **INT-01**: Migration + Seeding integration
  - Fresh migrations → Immediate seeding → Data validates correctly
  - No constraint violations or relationship issues

- **INT-02**: Cleanup + Health Check integration
  - Run job cleanup → Check system health → Verify no service degradation
  - Cleanup doesn't affect running services

---

## 5. PERFORMANCE & LOAD TEST SCENARIOS

### Database Performance

- **PERF-DB-01**: Large dataset migrations
  - Test migration performance with 100k+ records
  - Verify migration completes within reasonable timeframes

- **PERF-DB-02**: Concurrent constraint enforcement
  - Test business rule enforcement under high concurrency
  - Verify invariants maintained under load

### Script Performance

- **PERF-SCRIPT-01**: Bulk operations
  - Test cleanup script with 50k+ jobs
  - Verify memory usage stays bounded

- **PERF-SCRIPT-02**: Export large datasets
  - Test user export with extensive history
  - Verify export completes without timeout

---

## 6. SECURITY TEST SCENARIOS

### Authentication & Authorization

- **SEC-01**: API key validation
  - Test invalid/expired/revoked API keys rejected
  - Test scope enforcement works correctly

- **SEC-02**: URN ownership validation
  - Test cross-tenant access attempts blocked
  - Test privilege escalation attempts fail

### Data Protection

- **SEC-03**: SQL injection prevention
  - Test admin scripts resist SQL injection
  - Verify parameterized queries used throughout

- **SEC-04**: Credential handling
  - Test no credentials logged or exposed
  - Verify environment variable usage only

---

## TESTING IMPLEMENTATION PLAN

### Phase 1: Core Test Infrastructure

1. Set up pytest framework for admin scripts
2. Create database test fixtures
3. Implement migration testing framework
4. Set up infrastructure testing with Terratest

### Phase 2: Component Tests

1. Implement all database migration tests
2. Implement all admin script tests
3. Implement infrastructure validation tests
4. Add performance benchmarks

### Phase 3: Integration Tests

1. End-to-end workflow tests
2. Cross-component integration tests
3. Load testing scenarios
4. Security testing implementation

### Phase 4: CI/CD Integration

1. Automated test execution
2. Test reporting and coverage
3. Performance regression detection
4. Security scanning integration

This test strategy ensures comprehensive coverage of all components while following Rule 2: Tests Before Code.
