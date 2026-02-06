# Framecast API - Central Build System
# Following "P1: Just is the Frontend" - every task has a Just target
# https://github.com/casey/just

set dotenv-load := true

# Ensure Rust tools are available
export PATH := env_var('HOME') + "/.cargo/bin:" + env_var('PATH')

# Show all available commands
default:
    @just --list

# ============================================================================
# SETUP & INSTALLATION (Rule II: Dependencies)
# ============================================================================

# Install all required tools and dependencies from scratch
setup: install-tools install-rust-deps install-python-deps install-pre-commit precommit-install
    @echo "Setup complete! Run 'just dev' to start development environment."

# Install system tools (Rust, uv, OpenTofu, LocalStack, Docker, pipx)
install-tools:
    @echo "Installing required tools..."
    # Install Rust if not present
    @if ! command -v rustc >/dev/null 2>&1; then \
        echo "Installing Rust..."; \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
        . ~/.cargo/env; \
    fi
    # Install uv for Python package management
    @if ! command -v uv >/dev/null 2>&1; then \
        echo "Installing uv..."; \
        curl -LsSf https://astral.sh/uv/install.sh | sh; \
    fi
    # Install pipx for isolated Python tool installs
    @if ! command -v pipx >/dev/null 2>&1; then \
        echo "Installing pipx..."; \
        case "$$(uname -s)" in \
            Darwin*) brew install pipx ;; \
            *) sudo apt-get install -y pipx ;; \
        esac; \
        pipx ensurepath; \
    fi
    # Install OpenTofu for Infrastructure as Code
    @if ! command -v tofu >/dev/null 2>&1; then \
        echo "Installing OpenTofu..."; \
        case "$$(uname -s)" in \
            Darwin*) \
                if command -v brew >/dev/null 2>&1; then \
                    brew install opentofu; \
                else \
                    echo "Please install Homebrew first, then run 'brew install opentofu'"; \
                    exit 1; \
                fi ;; \
            *) echo "Please install OpenTofu manually for your platform"; exit 1 ;; \
        esac \
    fi
    # Install cargo-lambda for Lambda builds
    @if ! command -v cargo-lambda >/dev/null 2>&1; then \
        echo "Installing cargo-lambda..."; \
        cargo install cargo-lambda; \
    fi
    # Install LocalStack CLI
    @if ! command -v localstack >/dev/null 2>&1; then \
        echo "Installing LocalStack CLI..."; \
        pip3 install localstack[cli] 2>/dev/null || echo "LocalStack will be available via Docker"; \
    fi
    # Verify Docker is available
    @if ! command -v docker >/dev/null 2>&1; then \
        echo "Docker is required but not installed. Please install Docker Desktop."; \
        exit 1; \
    fi
    @echo "All tools installed successfully"

# Install Rust dependencies and update toolchain
install-rust-deps:
    @echo "Installing Rust dependencies..."
    rustup update
    rustup component add rustfmt clippy
    cargo install cargo-watch
    cargo install sqlx-cli --features postgres
    @echo "Rust dependencies installed"

# Install Python dependencies for E2E tests
install-python-deps:
    @echo "Installing Python dependencies for E2E tests..."
    cd tests/e2e && uv sync
    @echo "Python dependencies installed"

# Install pre-commit (requires pipx from install-tools)
install-pre-commit:
    @echo "Installing pre-commit..."
    pipx install pre-commit || pipx upgrade pre-commit
    @echo "Pre-commit installed"

# ============================================================================
# DEVELOPMENT ENVIRONMENT (Rules IV, VII, X: Backing Services, Port Binding, Dev/Prod Parity)
# ============================================================================

# Start complete local development environment
dev: start-backing-services start-api

# Start the API server in local development mode
start-api:
    @echo "Starting Framecast API server..."
    cargo run --bin local

# Start complete development environment (backing services + API)
start-full: start-backing-services
    @echo "Starting Framecast development environment..."
    @echo "Access points:"
    @echo "  API:          http://localhost:3000"
    @echo "  Inngest UI:   http://localhost:8288"
    @echo "  LocalStack:   http://localhost:4566"
    @echo ""
    @echo "Use 'just logs' to view service logs"
    @echo "Use 'just stop' to stop all services"
    @echo ""
    @echo "Starting API server..."
    just start-api

# Start backing services (LocalStack, Inngest, PostgreSQL)
start-backing-services:
    @echo "Starting backing services..."
    docker compose -f docker-compose.local.yml up -d --remove-orphans
    @echo "Waiting for services to be ready..."
    sleep 5
    just health-check
    just setup-localstack

# Stop all development services
stop:
    @echo "Stopping development services..."
    docker compose -f docker-compose.local.yml down

# View aggregated logs from all services
logs:
    docker compose -f docker-compose.local.yml logs -f

# Check health of all backing services
health-check:
    @echo "Checking service health..."
    @curl -s http://localhost:4566/_localstack/health || echo "LocalStack not ready"
    @curl -s http://localhost:8288/health || echo "Inngest not ready"
    @echo "Health check complete"

# Initialize LocalStack S3 buckets and services
setup-localstack:
    @echo "Setting up LocalStack S3 buckets..."
    # Wait for LocalStack to be ready
    @until curl -s http://localhost:4566/_localstack/health >/dev/null; do echo "Waiting for LocalStack..."; sleep 1; done
    # Create S3 buckets
    aws --endpoint-url=http://localhost:4566 s3 mb s3://framecast-outputs-dev || true
    aws --endpoint-url=http://localhost:4566 s3 mb s3://framecast-assets-dev || true
    @echo "LocalStack setup complete"

# ============================================================================
# DATABASE MANAGEMENT (Rules IV, XII: Backing Services, Admin Processes)
# ============================================================================

# Run pending database migrations
migrate:
    @echo "Running database migrations..."
    sqlx migrate run --database-url "${DATABASE_URL}"
    @echo "Migrations complete"

# Create a new migration file
migrate-new name:
    @echo "Creating new migration: {{name}}"
    sqlx migrate add "{{name}}" --source migrations

# Rollback last migration (USE WITH CAUTION)
migrate-rollback:
    @echo "Rolling back last migration..."
    sqlx migrate revert --database-url "${DATABASE_URL}"

# Check migration status
migrate-status:
    @echo "Migration status:"
    sqlx migrate info --database-url "${DATABASE_URL}"

# Reset database (DROP ALL DATA - development only)
migrate-reset:
    @echo "RESETTING DATABASE - THIS WILL DELETE ALL DATA!"
    @read -p "Are you sure? Type 'yes' to confirm: " confirm && [ "$$confirm" = "yes" ]
    dropdb framecast_dev || true
    createdb framecast_dev
    just migrate
    just seed

# Seed database with test data
seed:
    @echo "Seeding database with test data..."
    # TODO: Implement seeding script
    @echo "Database seeded"

# Generate sqlx offline query data for compile-time verification
sqlx-prepare:
    @echo "Generating sqlx offline query data..."
    cargo sqlx prepare --workspace
    @echo "sqlx offline data generated in .sqlx/"

# ============================================================================
# TESTING (Rules I, VI: Codebase, Processes)
# ============================================================================

# Run all Rust unit and integration tests
test *args="":
    @echo "Running Rust tests..."
    cargo test --workspace {{args}}

# Run tests with file watching for development
test-watch:
    @echo "Running tests with file watching..."
    cargo watch -x "test --workspace"

# Run all E2E tests in mocked mode (fast, CI-friendly)
test-e2e-mocked:
    @echo "Running E2E tests in mocked mode..."
    cd tests/e2e && uv run pytest tests/ -m "not real_services" --tb=short

# Run all E2E tests with real services (slower, pre-release)
test-e2e-real:
    @echo "Running E2E tests with real services..."
    @echo "This requires valid API credentials in .env"
    cd tests/e2e && uv run pytest tests/ --tb=short

# Run E2E tests with email verification
test-e2e-with-email:
    @echo "Running E2E tests with LocalStack email verification..."
    @echo "Starting LocalStack if needed..."
    @docker-compose -f docker-compose.localstack.yml up -d localstack --remove-orphans
    @echo "Waiting for LocalStack to be ready..."
    @sleep 15
    @echo "Setting up SES identities..."
    @./scripts/localstack-init/01-setup-ses.sh
    @echo "Running E2E tests with email verification..."
    cd tests/e2e && uv run pytest tests/test_invitation_workflow_e2e.py -v --tb=short
    @echo "E2E tests with email verification completed!"

# Run complete invitation workflow tests (Python E2E)
test-invitation-workflow:
    @echo "Running invitation workflow E2E tests..."
    @echo "Starting LocalStack if needed..."
    @docker-compose -f docker-compose.localstack.yml up -d localstack --remove-orphans
    @echo "Waiting for LocalStack to be ready..."
    @sleep 15
    @echo "Setting up SES identities..."
    @./scripts/localstack-init/01-setup-ses.sh
    @echo "Running Python E2E tests..."
    cd tests/e2e && uv run pytest tests/test_invitation_workflow_e2e.py -v --tb=short
    @echo "Invitation workflow E2E tests completed!"

# Start LocalStack services for testing
localstack-start:
    @echo "Starting LocalStack services..."
    docker-compose -f docker-compose.localstack.yml up -d --remove-orphans
    @echo "Waiting for services to initialize..."
    @sleep 15
    @echo "LocalStack services are ready!"
    @echo "Access points:"
    @echo "  LocalStack: http://localhost:4566"
    @echo "  MailHog UI: http://localhost:8025"
    @echo "  Test DB:   localhost:5433"

# Stop LocalStack services
localstack-stop:
    @echo "Stopping LocalStack services..."
    docker-compose -f docker-compose.localstack.yml down
    @echo "LocalStack services stopped!"

# Restart LocalStack services
localstack-restart: localstack-stop localstack-start

# View LocalStack service logs
localstack-logs:
    @echo "Viewing LocalStack logs..."
    docker-compose -f docker-compose.localstack.yml logs -f localstack

# Check LocalStack health
localstack-health:
    @echo "Checking LocalStack health..."
    @curl -s http://localhost:4566/_localstack/health | jq '.' || echo "LocalStack not responding"

# Run specific E2E test suites
test-e2e suite *args="":
    @echo "Running E2E test suite: {{suite}}"
    cd tests/e2e && uv run pytest tests/test_{{suite}}.py {{args}}

# Run performance and load tests
test-performance:
    @echo "Running performance tests..."
    cd tests/e2e && uv run pytest tests/test_performance.py -v

# ============================================================================
# CI PIPELINE (GitHub Actions)
# ============================================================================

# Run CI pipeline (formatting, linting, tests) - used by GitHub Actions
ci: fmt-check ci-clippy ci-test
    @echo "CI pipeline passed"

# Run clippy in CI mode (with SQLX_OFFLINE)
ci-clippy:
    @echo "Running Clippy linter (CI mode)..."
    SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings

# Run tests in CI mode (excludes integration tests, run separately with ci-test-integration)
ci-test:
    @echo "Running tests (CI mode)..."
    cargo test --workspace --exclude framecast-integration-tests

# Run integration tests in CI mode (requires PostgreSQL with migrations applied)
ci-test-integration:
    @echo "Running integration tests (CI mode)..."
    cargo test -p framecast-integration-tests -- --test-threads=1

# Run migrations in CI mode (requires DATABASE_URL)
ci-migrate:
    @echo "Running migrations (CI mode)..."
    sqlx migrate run

# Setup LocalStack S3 buckets for CI (reads AWS_ENDPOINT_URL, defaults to localhost)
ci-setup-localstack:
    #!/usr/bin/env bash
    set -e
    ENDPOINT="${AWS_ENDPOINT_URL:-http://localhost:4566}"
    echo "Setting up LocalStack S3 buckets (CI mode) at $ENDPOINT..."
    aws --endpoint-url="$ENDPOINT" s3 mb s3://framecast-outputs-dev || true
    aws --endpoint-url="$ENDPOINT" s3 mb s3://framecast-assets-dev || true

# Setup LocalStack SES identities for CI (reads AWS_ENDPOINT_URL, defaults to localhost)
ci-setup-ses:
    #!/usr/bin/env bash
    set -e
    ENDPOINT="${AWS_ENDPOINT_URL:-http://localhost:4566}"
    echo "Setting up LocalStack SES identities (CI mode) at $ENDPOINT..."
    echo "Waiting for LocalStack to be ready..."
    for i in $(seq 1 30); do
        if aws --endpoint-url="$ENDPOINT" ses list-identities --region us-east-1 > /dev/null 2>&1; then
            echo "✅ LocalStack is ready"
            break
        fi
        if [ "$i" = "30" ]; then
            echo "❌ LocalStack not ready after 30 attempts"
            exit 1
        fi
        echo "⏳ Waiting for LocalStack... (attempt $i/30)"
        sleep 2
    done
    EMAIL_ADDRESSES=(
        "invitations@framecast.app"
        "noreply@framecast.app"
        "support@framecast.app"
        "test@framecast.app"
        "invitee@example.com"
        "developer@example.com"
        "admin@example.com"
        "user@test.com"
        "owner-e2e@test.com"
        "invitee-e2e@test.com"
    )
    for email in "${EMAIL_ADDRESSES[@]}"; do
        echo "✉️ Verifying email identity: $email"
        aws --endpoint-url="$ENDPOINT" ses verify-email-identity \
            --email-address "$email" \
            --region us-east-1 || echo "Failed to verify $email"
    done
    echo "🔍 Listing verified identities..."
    aws --endpoint-url="$ENDPOINT" ses list-identities --region us-east-1
    echo "✅ SES setup completed!"

# Start API server in background for E2E tests (CI mode). Pass binary path to skip building.
ci-start-api binary_path="":
    #!/usr/bin/env bash
    set -e
    if [ -z "{{binary_path}}" ]; then
        echo "Building API server..."
        cargo build --bin local
        BINARY="./target/debug/local"
    else
        echo "Using pre-built binary: {{binary_path}}"
        chmod +x "{{binary_path}}"
        BINARY="{{binary_path}}"
    fi
    echo "Starting API server in background..."
    $BINARY &
    API_PID=$!
    echo "$API_PID" > /tmp/framecast-api.pid
    trap "kill $API_PID 2>/dev/null || true" EXIT
    echo "Waiting for API server to be ready (PID: $API_PID)..."
    for i in $(seq 1 30); do
        if curl -sf http://localhost:3000/health > /dev/null 2>&1; then
            echo "✅ API server is ready (PID: $API_PID)"
            trap - EXIT
            exit 0
        fi
        echo "⏳ Waiting for API server... (attempt $i/30)"
        sleep 2
    done
    echo "❌ API server did not start within 60 seconds"
    exit 1

# Stop API server started by ci-start-api
ci-stop-api:
    #!/usr/bin/env bash
    if [ -f /tmp/framecast-api.pid ]; then
        kill "$(cat /tmp/framecast-api.pid)" 2>/dev/null || true
        rm -f /tmp/framecast-api.pid
        echo "🛑 API server stopped"
    fi

# Run Python E2E tests in CI mode
ci-test-e2e:
    #!/usr/bin/env bash
    set -e
    echo "Running Python E2E tests (CI mode)..."
    cd tests/e2e && uv run pytest tests/ -m "real_services" -v --tb=short

# ============================================================================
# CODE QUALITY (Rules I, IX: Codebase, Disposability)
# ============================================================================

# Run all quality checks (formatting, linting, tests, pre-commit)
check: fmt-check clippy test precommit-run-all
    @echo "All quality checks passed"

# Check code formatting
fmt-check:
    @echo "Checking code formatting..."
    cargo fmt --all -- --check

# Format all code
fmt:
    @echo "Formatting code..."
    cargo fmt --all

# Run Clippy linter
clippy:
    @echo "Running Clippy linter..."
    cargo clippy --workspace --all-targets -- -D warnings

# Fix common linting issues automatically
fix:
    @echo "Fixing common issues..."
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
    cargo fmt --all

# ============================================================================
# PRE-COMMIT HOOKS (Code Quality & Security)
# ============================================================================

# Install pre-commit hooks into the git repository
precommit-install:
    @echo "Installing pre-commit hooks..."
    pre-commit install --install-hooks --hook-type pre-commit
    pre-commit install --hook-type pre-push
    pre-commit install --hook-type commit-msg
    @echo "Pre-commit hooks installed"

# Run pre-commit hooks on staged files
precommit-run:
    @echo "Running pre-commit hooks on staged files..."
    pre-commit run

# Run pre-commit hooks on all files
precommit-run-all:
    @echo "Running pre-commit hooks on all files..."
    pre-commit run --all-files

# Update pre-commit hooks to latest versions
precommit-update:
    @echo "Updating pre-commit hooks..."
    pre-commit autoupdate
    @echo "Pre-commit hooks updated"

# Run specific pre-commit hook
precommit-hook hook:
    @echo "Running specific hook: {{hook}}"
    pre-commit run {{hook}}

# Skip pre-commit hooks for emergency commits (use sparingly)
commit-emergency message:
    @echo "Emergency commit (skipping hooks): {{message}}"
    git commit --no-verify -m "{{message}}"

# ============================================================================
# BUILD (Rule V: Build, Release, Run)
# ============================================================================

# Build Lambda deployment package with cargo-lambda
lambda-build:
    @echo "Building Lambda with cargo-lambda..."
    SQLX_OFFLINE=true cargo lambda build --release --bin lambda --output-format zip
    @echo "Lambda built: target/lambda/lambda/bootstrap.zip"

# Watch mode for local Lambda development (hot reload)
lambda-watch:
    @echo "Starting cargo-lambda watch mode..."
    @echo "API will be available at http://localhost:9000"
    cargo lambda watch --bin lambda

# Build all release artifacts
build: lambda-build build-docker
    @echo "All artifacts built successfully"

# Build Docker images for RunPod workers
build-docker:
    @echo "Building Docker images..."
    docker build -t framecast/comfyui-worker:latest -f infra/runpod/Dockerfile .
    @echo "Docker images built"

# ============================================================================
# INFRASTRUCTURE (OpenTofu)
# ============================================================================

# Initialize OpenTofu (download providers)
infra-init:
    @echo "Initializing OpenTofu..."
    cd infra/opentofu && tofu init
    @echo "OpenTofu initialized"

# Validate OpenTofu configuration
infra-validate:
    @echo "Validating OpenTofu configuration..."
    cd infra/opentofu && tofu validate
    @echo "OpenTofu configuration is valid"

# Plan infrastructure changes for an environment
infra-plan env="dev":
    @echo "Planning infrastructure changes for {{env}}..."
    cd infra/opentofu && tofu plan -var-file=environments/{{env}}.tfvars

# Format OpenTofu files
infra-fmt:
    @echo "Formatting OpenTofu files..."
    cd infra/opentofu && tofu fmt -recursive
    @echo "OpenTofu files formatted"

# ============================================================================
# LOCAL DEPLOYMENT (LocalStack)
# ============================================================================

# Deploy full stack to LocalStack for testing
deploy-local: lambda-build
    @echo "Deploying to LocalStack..."
    @echo "Starting LocalStack if needed..."
    docker compose -f docker-compose.local.yml up -d localstack --remove-orphans
    @until curl -s http://localhost:4566/_localstack/health >/dev/null; do echo "Waiting for LocalStack..."; sleep 1; done
    @echo "Running OpenTofu apply..."
    cd infra/opentofu && tofu init -reconfigure && \
        TF_VAR_database_url="postgresql://postgres:postgres@host.docker.internal:5432/framecast_dev" \
        TF_VAR_jwt_secret="local-dev-jwt-secret" \
        tofu apply -var-file=environments/localstack.tfvars -auto-approve
    @echo "LocalStack deployment complete!"
    @echo "API endpoint: http://localhost:4566/restapis/.../dev/_user_request_/"

# Deploy only Lambda to LocalStack (faster iteration)
deploy-local-lambda: lambda-build
    @echo "Deploying Lambda to LocalStack..."
    @until curl -s http://localhost:4566/_localstack/health >/dev/null; do echo "Waiting for LocalStack..."; sleep 1; done
    # Update existing Lambda function
    aws --endpoint-url=http://localhost:4566 lambda update-function-code \
        --function-name framecast-dev-api \
        --zip-file fileb://target/lambda/lambda/bootstrap.zip || \
    echo "Lambda function not found. Run 'just deploy-local' first."
    @echo "Lambda updated"

# Destroy LocalStack resources
deploy-local-destroy:
    @echo "Destroying LocalStack resources..."
    cd infra/opentofu && \
        TF_VAR_database_url="dummy" \
        TF_VAR_jwt_secret="dummy" \
        tofu destroy -var-file=environments/localstack.tfvars -auto-approve
    @echo "LocalStack resources destroyed"

# Get LocalStack API endpoint
deploy-local-endpoint:
    @echo "Getting LocalStack API endpoint..."
    @aws --endpoint-url=http://localhost:4566 apigatewayv2 get-apis --query 'Items[0].ApiEndpoint' --output text 2>/dev/null || echo "No API found"

# ============================================================================
# AWS DEPLOYMENT (OpenTofu)
# ============================================================================

# Deploy to AWS dev environment
deploy-dev: lambda-build
    @echo "Deploying to AWS dev environment..."
    cd infra/opentofu && tofu init && \
        tofu apply -var-file=environments/dev.tfvars
    @echo "Dev deployment complete"

# Deploy to AWS staging environment
deploy-staging: lambda-build
    @echo "Deploying to AWS staging environment..."
    cd infra/opentofu && tofu init && \
        tofu apply -var-file=environments/staging.tfvars
    @echo "Staging deployment complete"

# Deploy to AWS production environment (runs tests first)
deploy-prod: test test-e2e-mocked lambda-build
    @echo "Deploying to AWS production environment..."
    @read -p "Deploy to PRODUCTION? Type 'yes' to confirm: " confirm && [ "$$confirm" = "yes" ]
    cd infra/opentofu && tofu init && \
        tofu apply -var-file=environments/prod.tfvars
    @echo "Production deployment complete"

# Destroy AWS environment (with confirmation)
deploy-destroy env="dev":
    @echo "Destroying AWS {{env}} environment..."
    @read -p "Destroy {{env}}? Type 'yes' to confirm: " confirm && [ "$$confirm" = "yes" ]
    cd infra/opentofu && tofu destroy -var-file=environments/{{env}}.tfvars
    @echo "{{env}} environment destroyed"

# Show deployment outputs
deploy-outputs env="dev":
    @echo "Deployment outputs for {{env}}:"
    cd infra/opentofu && tofu output

# View Lambda logs (CloudWatch)
logs-lambda env="dev":
    @echo "Viewing Lambda logs for framecast-{{env}}-api..."
    aws logs tail /aws/lambda/framecast-{{env}}-api --follow

# ============================================================================
# CI BASE IMAGE
# ============================================================================

# Build CI base image for amd64 (contains all tools pre-installed)
ci-image-build:
    @echo "Building CI base image for linux/amd64..."
    docker buildx build --platform linux/amd64 \
        -t ghcr.io/framecastdev/framecast-ci:latest \
        -f infra/ci/Dockerfile \
        --load .
    @echo "CI image built: ghcr.io/framecastdev/framecast-ci:latest"

# Push CI base image to GitHub Container Registry
ci-image-push:
    @echo "Building and pushing CI base image for linux/amd64..."
    docker buildx build --platform linux/amd64 \
        -t ghcr.io/framecastdev/framecast-ci:latest \
        -f infra/ci/Dockerfile \
        --push .
    @echo "CI image pushed to ghcr.io/framecastdev/framecast-ci:latest"

# Build and push CI image with a specific tag
ci-image-release tag:
    @echo "Building and pushing CI image with tag: {{tag}}..."
    docker buildx build --platform linux/amd64 \
        -t ghcr.io/framecastdev/framecast-ci:{{tag}} \
        -f infra/ci/Dockerfile \
        --push .
    @echo "CI image pushed: ghcr.io/framecastdev/framecast-ci:{{tag}}"

# ============================================================================
# ADMIN PROCESSES (Rule XII: Admin Processes)
# ============================================================================

# Generate a new API key for admin use
generate-api-key name:
    @echo "Generating API key for: {{name}}"
    # TODO: Implement API key generation
    @echo "API key generated"

# Clean up old job records and files (maintenance)
cleanup-jobs days="30":
    @echo "Cleaning up jobs older than {{days}} days..."
    # TODO: Implement cleanup script
    @echo "Cleanup complete"

# Archive completed jobs to cold storage
archive-jobs:
    @echo "Archiving completed jobs to cold storage..."
    # TODO: Implement archival script
    @echo "Jobs archived"

# Export user data for GDPR compliance
export-user-data user_id:
    @echo "Exporting data for user: {{user_id}}"
    # TODO: Implement user data export
    @echo "User data exported"

# ============================================================================
# DEVELOPMENT HELPERS
# ============================================================================

# Open documentation in browser
docs:
    @echo "Opening documentation..."
    open docs/spec/00_Index.md

# Show current environment configuration
env:
    @echo "Current environment configuration:"
    @echo "DATABASE_URL: ${DATABASE_URL:-Not set}"
    @echo "SUPABASE_URL: ${SUPABASE_URL:-Not set}"
    @echo "AWS_REGION: ${AWS_REGION:-Not set}"
    @echo "LOG_LEVEL: ${LOG_LEVEL:-info}"
    @echo "RUST_LOG: ${RUST_LOG:-framecast=debug}"

# Show system information and requirements
info:
    @echo "System Information:"
    @echo "Rust version: $(rustc --version 2>/dev/null || echo 'Not installed')"
    @echo "Python version: $(python3 --version 2>/dev/null || echo 'Not installed')"
    @echo "Docker version: $(docker --version 2>/dev/null || echo 'Not installed')"
    @echo "uv version: $(uv --version 2>/dev/null || echo 'Not installed')"
    @echo "OpenTofu version: $(tofu --version 2>/dev/null | head -1 || echo 'Not installed')"
    @echo "cargo-lambda version: $(cargo lambda --version 2>/dev/null || echo 'Not installed')"

# Reset everything and start fresh (DESTRUCTIVE)
reset-all:
    @echo "RESETTING ENTIRE DEVELOPMENT ENVIRONMENT"
    @echo "This will:"
    @echo "  - Stop all services"
    @echo "  - Remove all containers and volumes"
    @echo "  - Reset database"
    @echo "  - Clear target directory"
    @read -p "Are you sure? Type 'RESET' to confirm: " confirm && [ "$$confirm" = "RESET" ]
    just stop
    docker compose -f docker-compose.local.yml down -v --remove-orphans
    just migrate-reset
    cargo clean
    @echo "Environment reset complete. Run 'just setup && just dev' to restart."

# ============================================================================
# PROJECT INFORMATION
# ============================================================================

# Show project status and key metrics
status:
    @echo "Framecast API Project Status"
    @echo "=============================="
    @echo "Build System: Just $(just --version 2>/dev/null || echo 'Not found')"
    @echo "Workspace: $(find crates -name Cargo.toml | wc -l | tr -d ' ') crates"
    @echo "Migrations: $(find migrations -name '*.sql' 2>/dev/null | wc -l | tr -d ' ') files"
    @echo "Tests: $(find . -name '*.rs' -exec grep -l '#\[test\]' {} \; 2>/dev/null | wc -l | tr -d ' ') test files"
    @echo ""
    @echo "Quick Commands:"
    @echo "   just setup     - Install all dependencies"
    @echo "   just dev       - Start development environment"
    @echo "   just test      - Run all tests"
    @echo "   just check     - Run quality checks"

# Create release artifacts with version tag
release version:
    @echo "Creating release {{version}}..."
    git tag -a "v{{version}}" -m "Release v{{version}}"
    just build
    @echo "Release v{{version}} created"
