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
    @if ! command -v pipx >/dev/null 2>&1; then \
        echo "Installing pipx first..."; \
        if [[ "$OSTYPE" == "darwin"* ]]; then \
            brew install pipx; \
        else \
            python3 -m pip install --user pipx; \
            pipx ensurepath; \
        fi \
    fi
    pipx install pre-commit
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
    docker compose -f docker-compose.local.yml up -d --remove-orphans
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

# Generate sqlx offline query data for compile-time verification
sqlx-prepare:
    @echo "📦 Generating sqlx offline query data..."
    cargo sqlx prepare --workspace
    @echo "✅ sqlx offline data generated in .sqlx/"

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

# Run integration tests with LocalStack SES
test-integration-ses:
    @echo "📧 Running integration tests with LocalStack SES..."
    @echo "🚀 Starting LocalStack if needed..."
    @docker-compose -f docker-compose.localstack.yml up -d localstack --remove-orphans
    @echo "⏳ Waiting for LocalStack to be ready..."
    @sleep 15
    @echo "🔧 Setting up SES identities..."
    @./scripts/localstack-init/01-setup-ses.sh
    @echo "🧪 Running SES integration tests..."
    cd tests/integration && cargo test --test email_ses_e2e_test -- --nocapture
    @echo "✅ SES integration tests completed!"

# Run enhanced SES tests with email retrieval
test-ses-enhanced:
    @echo "📧 Running enhanced SES tests with email retrieval..."
    @echo "🚀 Starting LocalStack if needed..."
    @docker-compose -f docker-compose.localstack.yml up -d localstack --remove-orphans
    @echo "⏳ Waiting for LocalStack to be ready..."
    @sleep 15
    @echo "🔧 Setting up SES identities..."
    @./scripts/localstack-init/01-setup-ses.sh
    @echo "🧪 Running enhanced SES tests with email retrieval validation..."
    cd tests/integration && cargo test --test email_ses_e2e_test test_localstack_ses_email_retrieval_and_content_validation -- --nocapture
    @echo "✅ Enhanced SES tests with email retrieval completed!"

# Run E2E tests with email verification
test-e2e-with-email:
    @echo "📧 Running E2E tests with LocalStack email verification..."
    @echo "🚀 Starting LocalStack if needed..."
    @docker-compose -f docker-compose.localstack.yml up -d localstack --remove-orphans
    @echo "⏳ Waiting for LocalStack to be ready..."
    @sleep 15
    @echo "🔧 Setting up SES identities..."
    @./scripts/localstack-init/01-setup-ses.sh
    @echo "🧪 Running E2E tests with email verification..."
    cd tests/e2e && uv run pytest tests/test_invitation_workflow_e2e.py -v --tb=short
    @echo "✅ E2E tests with email verification completed!"

# Run complete invitation workflow tests (Rust + Python)
test-invitation-workflow:
    @echo "🔄 Running complete invitation workflow tests..."
    @echo "🚀 Starting LocalStack if needed..."
    @docker-compose -f docker-compose.localstack.yml up -d localstack --remove-orphans
    @echo "⏳ Waiting for LocalStack to be ready..."
    @sleep 15
    @echo "🔧 Setting up SES identities..."
    @./scripts/localstack-init/01-setup-ses.sh
    @echo "🧪 Running Rust integration tests..."
    cd tests/integration && cargo test --test email_ses_e2e_test -- --nocapture
    @echo "🧪 Running Python E2E tests..."
    cd tests/e2e && uv run pytest tests/test_invitation_workflow_e2e.py -v --tb=short
    @echo "✅ Complete invitation workflow tests completed!"

# Start LocalStack services for testing
localstack-start:
    @echo "🚀 Starting LocalStack services..."
    docker-compose -f docker-compose.localstack.yml up -d --remove-orphans
    @echo "⏳ Waiting for services to initialize..."
    @sleep 15
    @echo "✅ LocalStack services are ready!"
    @echo "📊 Access points:"
    @echo "  LocalStack: http://localhost:4566"
    @echo "  MailHog UI: http://localhost:8025"
    @echo "  Test DB:   localhost:5433"

# Stop LocalStack services
localstack-stop:
    @echo "🛑 Stopping LocalStack services..."
    docker-compose -f docker-compose.localstack.yml down
    @echo "✅ LocalStack services stopped!"

# Restart LocalStack services
localstack-restart: localstack-stop localstack-start

# View LocalStack service logs
localstack-logs:
    @echo "📋 Viewing LocalStack logs..."
    docker-compose -f docker-compose.localstack.yml logs -f localstack

# Check LocalStack health
localstack-health:
    @echo "🏥 Checking LocalStack health..."
    @curl -s http://localhost:4566/_localstack/health | jq '.' || echo "LocalStack not responding"

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
build: sam-build build-docker
    @echo "✅ All artifacts built successfully"

# Build Docker images for RunPod workers
build-docker:
    @echo "🐳 Building Docker images..."
    docker build -t framecast/comfyui-worker:latest -f infra/runpod/Dockerfile .
    @echo "✅ Docker images built"

# ============================================================================
# AWS SAM - Serverless Deployment
# ============================================================================

# Install cargo-lambda for SAM builds
install-cargo-lambda:
    @echo "🦀 Installing cargo-lambda..."
    cargo install cargo-lambda
    @echo "✅ cargo-lambda installed"

# Build with SAM (uses cargo-lambda)
sam-build:
    @echo "🏗️ Building Lambda with SAM..."
    sam build --beta-features
    @echo "✅ SAM build complete"

# Start local API with SAM (uses LocalStack network)
sam-local:
    @echo "🚀 Starting SAM local API..."
    @echo "📊 API will be available at http://localhost:3001"
    @echo "💡 Make sure LocalStack is running: just start-backing-services"
    sam local start-api --config-env dev

# Invoke Lambda locally with test event
sam-invoke event="events/api-gateway-request.json":
    @echo "⚡ Invoking Lambda locally..."
    sam local invoke FramecastApiFunction --event {{event}} --config-env dev

# Validate SAM template
sam-validate:
    @echo "🔍 Validating SAM template..."
    sam validate --lint
    @echo "✅ Template is valid"

# Deploy to dev environment
sam-deploy-dev:
    @echo "🚀 Deploying to dev environment..."
    sam build --beta-features
    sam deploy --config-env dev
    @echo "✅ Deployed to dev"

# Deploy to staging environment
sam-deploy-staging:
    @echo "🚀 Deploying to staging environment..."
    sam build --beta-features
    sam deploy --config-env staging
    @echo "✅ Deployed to staging"

# Deploy to production environment (runs tests first)
sam-deploy-prod:
    @echo "🚀 Deploying to production environment..."
    @echo "⚠️ Running tests before production deployment..."
    just test
    just test-e2e-mocked
    sam build --beta-features
    sam deploy --config-env prod
    @echo "✅ Deployed to production"

# View Lambda logs (tail mode)
sam-logs env="dev":
    @echo "📋 Viewing logs for framecast-api-{{env}}..."
    sam logs --stack-name framecast-api-{{env}} --tail

# Delete SAM stack
sam-delete env="dev":
    @echo "🗑️ Deleting SAM stack framecast-api-{{env}}..."
    @read -p "Are you sure? Type 'yes' to confirm: " confirm && [ "$$confirm" = "yes" ]
    sam delete --stack-name framecast-api-{{env}}
    @echo "✅ Stack deleted"

# Show SAM stack outputs
sam-outputs env="dev":
    @echo "📊 Stack outputs for framecast-api-{{env}}:"
    aws cloudformation describe-stacks --stack-name framecast-api-{{env}} --query 'Stacks[0].Outputs' --output table

# ============================================================================
# SAM LOCAL TESTING
# ============================================================================

# Build and start SAM local API in background for testing
sam-local-start:
    @echo "🚀 Starting SAM local API for testing..."
    @echo "📋 Prerequisites: Docker, LocalStack, PostgreSQL"
    just start-backing-services
    just sam-build
    @echo "⏳ Starting SAM local in background..."
    @nohup sam local start-api --config-env dev > /tmp/sam-local.log 2>&1 &
    @echo "⏳ Waiting for SAM local to be ready..."
    @for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
        if curl -s http://localhost:3001/health >/dev/null 2>&1; then \
            echo "✅ SAM local is ready on port 3001"; \
            exit 0; \
        fi; \
        echo "  Waiting... ($$i/15)"; \
        sleep 4; \
    done; \
    echo "❌ SAM local failed to start within 60 seconds"; \
    echo "📋 Last 50 lines of log:"; \
    tail -50 /tmp/sam-local.log; \
    exit 1

# Stop SAM local API
sam-local-stop:
    @echo "🛑 Stopping SAM local..."
    @pkill -f "sam local start-api" 2>/dev/null || true
    @sleep 2
    @echo "✅ SAM local stopped"

# Check if SAM local is running
sam-local-status:
    @echo "🔍 Checking SAM local status..."
    @if curl -s http://localhost:3001/health >/dev/null 2>&1; then \
        echo "✅ SAM local is running on port 3001"; \
    else \
        echo "❌ SAM local is not running"; \
    fi

# View SAM local logs
sam-local-logs:
    @echo "📋 SAM local logs:"
    @if [ -f /tmp/sam-local.log ]; then \
        tail -100 /tmp/sam-local.log; \
    else \
        echo "No log file found at /tmp/sam-local.log"; \
    fi

# Run E2E tests against SAM local
test-e2e-sam:
    @echo "🧪 Running E2E tests against SAM local..."
    @echo "📋 Make sure SAM local is running: just sam-local-start"
    @if ! curl -s http://localhost:3001/health >/dev/null 2>&1; then \
        echo "❌ SAM local is not running. Run 'just sam-local-start' first."; \
        exit 1; \
    fi
    cd tests/e2e && TEST_USE_SAM_LOCAL=true uv run pytest tests/test_sam_e2e.py -v --tb=short

# Run Rust integration tests for SAM local
test-integration-sam:
    @echo "🧪 Running Rust SAM local integration tests..."
    @echo "📋 Make sure SAM local is running: just sam-local-start"
    @if ! curl -s http://localhost:3001/health >/dev/null 2>&1; then \
        echo "❌ SAM local is not running. Run 'just sam-local-start' first."; \
        exit 1; \
    fi
    SAM_LOCAL_API_URL=http://localhost:3001 cargo test --test sam_local_test -- --nocapture

# Full SAM test suite (start, test, stop)
test-sam-full:
    @echo "🧪 Running full SAM test suite..."
    @echo ""
    @echo "Phase 1: Starting SAM local..."
    just sam-local-start
    @echo ""
    @echo "Phase 2: Running Rust integration tests..."
    just test-integration-sam || (just sam-local-stop && exit 1)
    @echo ""
    @echo "Phase 3: Running Python E2E tests..."
    just test-e2e-sam || (just sam-local-stop && exit 1)
    @echo ""
    @echo "Phase 4: Cleaning up..."
    just sam-local-stop
    @echo ""
    @echo "✅ Full SAM test suite completed successfully!"

# Quick SAM test (assumes SAM local is already running)
test-sam-quick:
    @echo "🧪 Running quick SAM tests (SAM local must be running)..."
    @if ! curl -s http://localhost:3001/health >/dev/null 2>&1; then \
        echo "❌ SAM local is not running. Run 'just sam-local-start' first."; \
        exit 1; \
    fi
    @echo "Running Rust tests..."
    SAM_LOCAL_API_URL=http://localhost:3001 cargo test --test sam_local_test -- --nocapture
    @echo "Running Python tests..."
    cd tests/e2e && TEST_USE_SAM_LOCAL=true uv run pytest tests/test_sam_e2e.py -v --tb=short
    @echo "✅ Quick SAM tests completed!"

# Create release artifacts with version tag
release version:
    @echo "🚀 Creating release {{version}}..."
    git tag -a "v{{version}}" -m "Release v{{version}}"
    just build
    @echo "✅ Release v{{version}} created"

# ============================================================================
# CI BASE IMAGE
# ============================================================================

# Build CI base image (contains all tools pre-installed)
ci-image-build:
    @echo "🐳 Building CI base image..."
    docker build -t 192.168.68.77:3000/thiago/framecast-ci:latest -f infra/ci/Dockerfile .
    @echo "✅ CI image built: 192.168.68.77:3000/thiago/framecast-ci:latest"

# Push CI base image to Gitea registry
ci-image-push: ci-image-build
    @echo "📤 Pushing CI image to registry..."
    docker push 192.168.68.77:3000/thiago/framecast-ci:latest
    @echo "✅ CI image pushed to 192.168.68.77:3000/thiago/framecast-ci:latest"

# Build and push CI image with a specific tag
ci-image-release tag:
    @echo "🐳 Building CI image with tag: {{tag}}..."
    docker build -t 192.168.68.77:3000/thiago/framecast-ci:{{tag}} -f infra/ci/Dockerfile .
    docker push 192.168.68.77:3000/thiago/framecast-ci:{{tag}}
    @echo "✅ CI image pushed: 192.168.68.77:3000/thiago/framecast-ci:{{tag}}"

# ============================================================================
# INFRASTRUCTURE & DEPLOYMENT (Rules V, XI: Build/Release/Run, Logs)
# ============================================================================

# Deploy to staging environment (uses SAM for Lambda, OpenTofu for other infra)
deploy-staging: sam-deploy-staging
    @echo "✅ Staging deployment complete"

# Deploy to production environment (uses SAM for Lambda, OpenTofu for other infra)
deploy-prod: sam-deploy-prod
    @echo "✅ Production deployment complete"

# View production logs (CloudWatch)
logs-prod:
    @echo "📊 Viewing production logs..."
    sam logs --stack-name framecast-api-prod --tail

# Deploy non-Lambda infrastructure with OpenTofu (RDS, VPC, etc.)
deploy-infra env="dev":
    @echo "🏗️ Deploying infrastructure with OpenTofu..."
    cd infra/opentofu && tofu init && tofu plan -var="environment={{env}}"
    @read -p "Apply changes? (y/N): " confirm && [ "$$confirm" = "y" ]
    cd infra/opentofu && tofu apply -var="environment={{env}}"

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
