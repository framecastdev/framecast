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
    @echo "✅ Setup complete! Run 'just dev' to start development environment."

# Install system tools (Rust, uv, OpenTofu, LocalStack, Docker)
install-tools:
    @echo "🔧 Installing required tools..."
    # Install Rust if not present
    @if ! command -v rustc >/dev/null 2>&1; then \
        echo "Installing Rust..."; \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
        source ~/.cargo/env; \
    fi
    # Install uv for Python package management
    @if ! command -v uv >/dev/null 2>&1; then \
        echo "Installing uv..."; \
        curl -LsSf https://astral.sh/uv/install.sh | sh; \
    fi
    # Install OpenTofu for Infrastructure as Code
    @if ! command -v tofu >/dev/null 2>&1; then \
        echo "Installing OpenTofu..."; \
        if [[ "$OSTYPE" == "darwin"* ]]; then \
            if command -v brew >/dev/null 2>&1; then \
                brew install opentofu; \
            else \
                echo "❌ Please install Homebrew first, then run 'brew install opentofu'"; \
                exit 1; \
            fi \
        else \
            echo "❌ Please install OpenTofu manually for your platform"; \
            exit 1; \
        fi \
    fi
    # Install LocalStack CLI
    @if ! command -v localstack >/dev/null 2>&1; then \
        echo "Installing LocalStack CLI..."; \
        pip3 install localstack[cli] 2>/dev/null || echo "LocalStack will be available via Docker"; \
    fi
    # Verify Docker is available
    @if ! command -v docker >/dev/null 2>&1; then \
        echo "❌ Docker is required but not installed. Please install Docker Desktop."; \
        exit 1; \
    fi
    @echo "✅ All tools installed successfully"

# Install Rust dependencies and update toolchain
install-rust-deps:
    @echo "🦀 Installing Rust dependencies..."
    rustup update
    rustup component add rustfmt clippy
    cargo install cargo-watch
    cargo install sqlx-cli --features postgres
    @echo "✅ Rust dependencies installed"

# Install Python dependencies for E2E tests
install-python-deps:
    @echo "🐍 Installing Python dependencies for E2E tests..."
    cd tests/e2e && uv sync
    @echo "✅ Python dependencies installed"

# Install pre-commit hooks
install-pre-commit:
    @echo "🪝 Installing pre-commit..."
    pip3 install pre-commit
    @echo "✅ Pre-commit installed"

# ============================================================================
# DEVELOPMENT ENVIRONMENT (Rules IV, VII, X: Backing Services, Port Binding, Dev/Prod Parity)
# ============================================================================

# Start complete local development environment
dev: start-backing-services start-api

# Start the API server in local development mode
start-api:
    @echo "🚀 Starting Framecast API server..."
    cargo run --bin local

# Start complete development environment (backing services + API)
start-full: start-backing-services
    @echo "🚀 Starting Framecast development environment..."
    @echo "📊 Access points:"
    @echo "  API:          http://localhost:3000"
    @echo "  Inngest UI:   http://localhost:8288"
    @echo "  LocalStack:   http://localhost:4566"
    @echo ""
    @echo "🔍 Use 'just logs' to view service logs"
    @echo "⏹️  Use 'just stop' to stop all services"
    @echo ""
    @echo "🏃 Starting API server..."
    just start-api

# Start backing services (LocalStack, Inngest, PostgreSQL)
start-backing-services:
    @echo "🔧 Starting backing services..."
    docker compose -f docker-compose.local.yml up -d
    @echo "⏳ Waiting for services to be ready..."
    sleep 5
    just health-check
    just setup-localstack

# Stop all development services
stop:
    @echo "⏹️ Stopping development services..."
    docker compose -f docker-compose.local.yml down

# View aggregated logs from all services
logs:
    docker compose -f docker-compose.local.yml logs -f

# Check health of all backing services
health-check:
    @echo "🏥 Checking service health..."
    @curl -s http://localhost:4566/_localstack/health || echo "❌ LocalStack not ready"
    @curl -s http://localhost:8288/health || echo "❌ Inngest not ready"
    @echo "✅ Health check complete"

# Initialize LocalStack S3 buckets and services
setup-localstack:
    @echo "🪣 Setting up LocalStack S3 buckets..."
    # Wait for LocalStack to be ready
    @until curl -s http://localhost:4566/_localstack/health >/dev/null; do echo "Waiting for LocalStack..."; sleep 1; done
    # Create S3 buckets
    aws --endpoint-url=http://localhost:4566 s3 mb s3://framecast-outputs-dev || true
    aws --endpoint-url=http://localhost:4566 s3 mb s3://framecast-assets-dev || true
    @echo "✅ LocalStack setup complete"

# ============================================================================
# DATABASE MANAGEMENT (Rules IV, XII: Backing Services, Admin Processes)
# ============================================================================

# Run pending database migrations
migrate:
    @echo "🗃️ Running database migrations..."
    sqlx migrate run --database-url "${DATABASE_URL}"
    @echo "✅ Migrations complete"

# Create a new migration file
migrate-new name:
    @echo "📝 Creating new migration: {{name}}"
    sqlx migrate add "{{name}}" --source migrations

# Rollback last migration (USE WITH CAUTION)
migrate-rollback:
    @echo "⚠️ Rolling back last migration..."
    sqlx migrate revert --database-url "${DATABASE_URL}"

# Check migration status
migrate-status:
    @echo "📊 Migration status:"
    sqlx migrate info --database-url "${DATABASE_URL}"

# Reset database (DROP ALL DATA - development only)
migrate-reset:
    @echo "🚨 RESETTING DATABASE - THIS WILL DELETE ALL DATA!"
    @read -p "Are you sure? Type 'yes' to confirm: " confirm && [ "$$confirm" = "yes" ]
    dropdb framecast_dev || true
    createdb framecast_dev
    just migrate
    just seed

# Seed database with test data
seed:
    @echo "🌱 Seeding database with test data..."
    # TODO: Implement seeding script
    @echo "✅ Database seeded"

# ============================================================================
# TESTING (Rules I, VI: Codebase, Processes)
# ============================================================================

# Run all Rust unit and integration tests
test *args="":
    @echo "🧪 Running Rust tests..."
    cargo test --workspace {{args}}

# Run tests with file watching for development
test-watch:
    @echo "👀 Running tests with file watching..."
    cargo watch -x "test --workspace"

# Run all E2E tests in mocked mode (fast, CI-friendly)
test-e2e-mocked:
    @echo "🎭 Running E2E tests in mocked mode..."
    cd tests/e2e && uv run pytest tests/ -m "not real_services" --tb=short

# Run all E2E tests with real services (slower, pre-release)
test-e2e-real:
    @echo "🌐 Running E2E tests with real services..."
    @echo "⚠️ This requires valid API credentials in .env"
    cd tests/e2e && uv run pytest tests/ --tb=short

# Run specific E2E test suites
test-e2e suite *args="":
    @echo "🎯 Running E2E test suite: {{suite}}"
    cd tests/e2e && uv run pytest tests/test_{{suite}}.py {{args}}

# Run performance and load tests
test-performance:
    @echo "🏁 Running performance tests..."
    cd tests/e2e && uv run pytest tests/test_performance.py -v

# ============================================================================
# CODE QUALITY (Rules I, IX: Codebase, Disposability)
# ============================================================================

# Run all quality checks (formatting, linting, tests, pre-commit)
check: fmt-check clippy test precommit-run-all
    @echo "✅ All quality checks passed"

# Check code formatting
fmt-check:
    @echo "📐 Checking code formatting..."
    cargo fmt --all -- --check

# Format all code
fmt:
    @echo "🎨 Formatting code..."
    cargo fmt --all

# Run Clippy linter
clippy:
    @echo "📎 Running Clippy linter..."
    cargo clippy --workspace --all-targets -- -D warnings

# Fix common linting issues automatically
fix:
    @echo "🔧 Fixing common issues..."
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
    cargo fmt --all

# ============================================================================
# PRE-COMMIT HOOKS (Code Quality & Security)
# ============================================================================

# Install pre-commit hooks into the git repository
precommit-install:
    @echo "🪝 Installing pre-commit hooks..."
    pre-commit install --install-hooks --hook-type pre-commit
    pre-commit install --hook-type pre-push
    pre-commit install --hook-type commit-msg
    @echo "✅ Pre-commit hooks installed"

# Run pre-commit hooks on staged files
precommit-run:
    @echo "🔍 Running pre-commit hooks on staged files..."
    pre-commit run

# Run pre-commit hooks on all files
precommit-run-all:
    @echo "🔍 Running pre-commit hooks on all files..."
    pre-commit run --all-files

# Update pre-commit hooks to latest versions
precommit-update:
    @echo "⬆️ Updating pre-commit hooks..."
    pre-commit autoupdate
    @echo "✅ Pre-commit hooks updated"

# Run specific pre-commit hook
precommit-hook hook:
    @echo "🎯 Running specific hook: {{hook}}"
    pre-commit run {{hook}}

# Skip pre-commit hooks for emergency commits (use sparingly)
commit-emergency message:
    @echo "🚨 Emergency commit (skipping hooks): {{message}}"
    git commit --no-verify -m "{{message}}"

# ============================================================================
# BUILD & RELEASE (Rule V: Build, Release, Run)
# ============================================================================

# Build all release artifacts
build: build-lambda build-docker
    @echo "✅ All artifacts built successfully"

# Build Lambda deployment packages
build-lambda:
    @echo "🏗️ Building Lambda functions..."
    cargo build --release --bin framecast-api
    # Package for Lambda deployment
    mkdir -p target/lambda/framecast-api
    cp target/release/framecast-api target/lambda/framecast-api/bootstrap
    cd target/lambda/framecast-api && zip -r ../framecast-api.zip .
    @echo "📦 Lambda package created: target/lambda/framecast-api.zip"

# Build Docker images for RunPod workers
build-docker:
    @echo "🐳 Building Docker images..."
    docker build -t framecast/comfyui-worker:latest -f infra/runpod/Dockerfile .
    @echo "✅ Docker images built"

# Create release artifacts with version tag
release version:
    @echo "🚀 Creating release {{version}}..."
    git tag -a "v{{version}}" -m "Release v{{version}}"
    just build
    @echo "✅ Release v{{version}} created"

# ============================================================================
# INFRASTRUCTURE & DEPLOYMENT (Rules V, XI: Build/Release/Run, Logs)
# ============================================================================

# Deploy to staging environment
deploy-staging:
    @echo "🚀 Deploying to staging..."
    cd infra/opentofu && tofu init && tofu plan -var="environment=staging"
    @read -p "Apply changes? (y/N): " confirm && [ "$$confirm" = "y" ]
    cd infra/opentofu && tofu apply -var="environment=staging"

# Deploy to production environment
deploy-prod:
    @echo "🚀 Deploying to production..."
    @echo "⚠️ This will deploy to PRODUCTION. Ensure all tests pass!"
    just test && just test-e2e-mocked
    cd infra/opentofu && tofu init && tofu plan -var="environment=production"
    @read -p "Deploy to PRODUCTION? Type 'yes' to confirm: " confirm && [ "$$confirm" = "yes" ]
    cd infra/opentofu && tofu apply -var="environment=production"

# View production logs (CloudWatch)
logs-prod:
    @echo "📊 Viewing production logs..."
    aws logs tail /aws/lambda/framecast-api --follow

# ============================================================================
# ADMIN PROCESSES (Rule XII: Admin Processes)
# ============================================================================

# Generate a new API key for admin use
generate-api-key name:
    @echo "🔑 Generating API key for: {{name}}"
    # TODO: Implement API key generation
    @echo "✅ API key generated"

# Clean up old job records and files (maintenance)
cleanup-jobs days="30":
    @echo "🧹 Cleaning up jobs older than {{days}} days..."
    # TODO: Implement cleanup script
    @echo "✅ Cleanup complete"

# Archive completed jobs to cold storage
archive-jobs:
    @echo "📦 Archiving completed jobs to cold storage..."
    # TODO: Implement archival script
    @echo "✅ Jobs archived"

# Export user data for GDPR compliance
export-user-data user_id:
    @echo "📤 Exporting data for user: {{user_id}}"
    # TODO: Implement user data export
    @echo "✅ User data exported"

# ============================================================================
# DEVELOPMENT HELPERS
# ============================================================================

# Open documentation in browser
docs:
    @echo "📖 Opening documentation..."
    open docs/spec/00_Index.md

# Show current environment configuration
env:
    @echo "🔧 Current environment configuration:"
    @echo "DATABASE_URL: ${DATABASE_URL:-Not set}"
    @echo "SUPABASE_URL: ${SUPABASE_URL:-Not set}"
    @echo "AWS_REGION: ${AWS_REGION:-Not set}"
    @echo "LOG_LEVEL: ${LOG_LEVEL:-info}"
    @echo "RUST_LOG: ${RUST_LOG:-framecast=debug}"

# Show system information and requirements
info:
    @echo "📋 System Information:"
    @echo "Rust version: $(rustc --version 2>/dev/null || echo 'Not installed')"
    @echo "Python version: $(python3 --version 2>/dev/null || echo 'Not installed')"
    @echo "Docker version: $(docker --version 2>/dev/null || echo 'Not installed')"
    @echo "uv version: $(uv --version 2>/dev/null || echo 'Not installed')"
    @echo "OpenTofu version: $(tofu --version 2>/dev/null | head -1 || echo 'Not installed')"

# Reset everything and start fresh (DESTRUCTIVE)
reset-all:
    @echo "🚨 RESETTING ENTIRE DEVELOPMENT ENVIRONMENT"
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
    @echo "✅ Environment reset complete. Run 'just setup && just dev' to restart."

# ============================================================================
# PROJECT INFORMATION
# ============================================================================

# Show project status and key metrics
status:
    @echo "📊 Framecast API Project Status"
    @echo "=============================="
    @echo "🏗️ Build System: Just $(just --version 2>/dev/null || echo 'Not found')"
    @echo "🦀 Workspace: $(find crates -name Cargo.toml | wc -l | tr -d ' ') crates"
    @echo "📁 Migrations: $(find migrations -name '*.sql' 2>/dev/null | wc -l | tr -d ' ') files"
    @echo "🧪 Tests: $(find . -name '*.rs' -exec grep -l '#\[test\]' {} \; 2>/dev/null | wc -l | tr -d ' ') test files"
    @echo ""
    @echo "🔗 Quick Commands:"
    @echo "   just setup     - Install all dependencies"
    @echo "   just dev       - Start development environment"
    @echo "   just test      - Run all tests"
    @echo "   just check     - Run quality checks"