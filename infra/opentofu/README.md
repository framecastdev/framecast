# Framecast Infrastructure - OpenTofu

OpenTofu configuration for deploying Framecast API to AWS.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Local Development                        │
├─────────────────────────────────────────────────────────────┤
│  just dev              →  Axum server on localhost:3000     │
│  just lambda-watch     →  cargo-lambda with hot reload      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              Local Deploy (LocalStack)                      │
├─────────────────────────────────────────────────────────────┤
│  just deploy-local     →  Full stack on LocalStack          │
│  - Lambda function deployed to LocalStack                   │
│  - API Gateway created on LocalStack                        │
│  - S3 buckets provisioned                                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  AWS Deploy (OpenTofu)                      │
├─────────────────────────────────────────────────────────────┤
│  just deploy-dev       →  Deploy to AWS dev environment     │
│  just deploy-staging   →  Deploy to AWS staging             │
│  just deploy-prod      →  Deploy to AWS production          │
└─────────────────────────────────────────────────────────────┘
```

## Structure

```
infra/opentofu/
├── main.tf                    # Root configuration, module imports
├── variables.tf               # Input variables
├── outputs.tf                 # Output values (API endpoint, etc.)
├── versions.tf                # Provider requirements
├── environments/
│   ├── dev.tfvars             # Development environment
│   ├── staging.tfvars         # Staging environment
│   ├── prod.tfvars            # Production environment
│   └── localstack.tfvars      # LocalStack (local testing)
└── modules/
    ├── lambda/main.tf         # Lambda + IAM + CloudWatch logs
    ├── api-gateway/main.tf    # HTTP API v2 + CORS + routes
    ├── s3/main.tf             # Outputs + Assets buckets
    └── monitoring/main.tf     # CloudWatch Alarms (prod only)
```

## Quick Start

### Prerequisites

- OpenTofu installed (`just install-tools`)
- cargo-lambda installed (`cargo install cargo-lambda`)
- AWS CLI configured (for AWS deployments)
- Docker running (for LocalStack)

### Local Development

```bash
# Start local API server (no Lambda, direct Axum)
just dev

# Or use cargo-lambda watch for Lambda simulation
just lambda-watch
```

### LocalStack Deployment

Test the full Lambda + API Gateway stack locally:

```bash
# Build Lambda and deploy to LocalStack
just deploy-local

# Get the API endpoint
just deploy-local-endpoint

# Test health endpoint
curl http://localhost:4566/restapis/<api-id>/dev/_user_request_/health
```

### AWS Deployment

```bash
# Set required environment variables
export TF_VAR_database_url="postgresql://..."
export TF_VAR_jwt_secret="your-jwt-secret"
# ... other secrets ...

# Deploy to dev
just deploy-dev

# Deploy to production (runs tests first)
just deploy-prod
```

## Environment Variables

Secrets should be passed via `TF_VAR_*` environment variables:

| Variable | Description | Required |
|----------|-------------|----------|
| `TF_VAR_database_url` | PostgreSQL connection URL | Yes |
| `TF_VAR_jwt_secret` | JWT signing secret | Yes |
| `TF_VAR_supabase_url` | Supabase project URL | No |
| `TF_VAR_supabase_anon_key` | Supabase anonymous key | No |
| `TF_VAR_anthropic_api_key` | Anthropic API key | No |
| `TF_VAR_inngest_event_key` | Inngest event key | No |
| `TF_VAR_inngest_signing_key` | Inngest signing key | No |
| `TF_VAR_runpod_api_key` | RunPod API key | No |
| `TF_VAR_runpod_endpoint_id` | RunPod endpoint ID | No |

## Modules

### Lambda Module (`modules/lambda/`)

Creates:
- Lambda function with provided.al2023 runtime
- IAM execution role with CloudWatch and S3 permissions
- CloudWatch Log Group

### API Gateway Module (`modules/api-gateway/`)

Creates:
- HTTP API v2 with Lambda integration
- CORS configuration
- Auto-deploy stage
- Access logging to CloudWatch

### S3 Module (`modules/s3/`)

Creates:
- Outputs bucket (for generated videos)
- Assets bucket (for user uploads)
- Lifecycle policies
- CORS configuration for assets

### Monitoring Module (`modules/monitoring/`)

Creates (production only):
- Lambda errors alarm
- API Gateway 5xx alarm
- Lambda duration alarm
- Lambda throttles alarm

## CI/CD

The GitHub Actions workflows handle deployment:

- **ci.yml**: Validates OpenTofu, builds Lambda, runs tests
- **deploy.yml**: Deploys to AWS environments

### Required Secrets

Set these in GitHub repository settings:

- `AWS_DEPLOY_ROLE_ARN`: IAM role ARN for OIDC authentication
- `DATABASE_URL`, `JWT_SECRET`, etc.: Environment secrets

## Commands Reference

```bash
# Infrastructure management
just infra-init           # Initialize OpenTofu
just infra-validate       # Validate configuration
just infra-plan dev       # Plan changes for dev
just infra-fmt            # Format .tf files

# Local deployment
just deploy-local         # Full deploy to LocalStack
just deploy-local-lambda  # Update Lambda only
just deploy-local-destroy # Tear down LocalStack resources
just deploy-local-endpoint # Get API endpoint

# AWS deployment
just deploy-dev           # Deploy to dev
just deploy-staging       # Deploy to staging
just deploy-prod          # Deploy to production (with tests)
just deploy-destroy dev   # Destroy environment
just deploy-outputs       # Show deployment outputs

# Logs
just logs-lambda dev      # Tail Lambda logs
```

## Security

- S3 buckets have public access blocked
- Lambda functions use least-privilege IAM roles
- Secrets passed via environment variables
- All resources are tagged for cost tracking

## Monitoring (Production)

- CloudWatch alarms for Lambda errors
- CloudWatch alarms for API Gateway 5xx errors
- Lambda duration and throttle alarms
- Structured logging to CloudWatch Logs
