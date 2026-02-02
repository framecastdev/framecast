# Framecast Infrastructure

OpenTofu configuration for AWS infrastructure following 12-Factor principles.

## Architecture

- **Lambda Functions**: Stateless API handlers (Rule VI)
- **API Gateway**: HTTP endpoint exposure (Rule VII)
- **S3 Buckets**: Object storage for outputs and assets (Rule IV)
- **RDS PostgreSQL**: Database (optional, can use Supabase instead) (Rule IV)
- **CloudWatch**: Logging and monitoring (Rule XI)
- **Secrets Manager**: Sensitive configuration (Rule III)

## Environments

- `dev` - Development environment
- `staging` - Staging environment
- `prod` - Production environment

## Configuration

Environment-specific variables are in `environments/` directory.

Sensitive variables (API keys, passwords) should be set via:

- Environment variables (local development)
- CI/CD secrets (production deployments)
- AWS Systems Manager Parameter Store
- External secret management systems

## Usage

### Prerequisites

1. Install OpenTofu: `brew install opentofu` (macOS)
2. Configure AWS credentials: `aws configure` or set environment variables
3. Build Lambda deployment packages: `just build-lambda`

### Deploy Infrastructure

```bash
# Initialize OpenTofu
cd infra/opentofu
tofu init

# Plan deployment (development)
tofu plan -var-file=environments/dev.tfvars

# Apply changes (development)
tofu apply -var-file=environments/dev.tfvars

# Plan for production
tofu plan -var-file=environments/prod.tfvars \
  -var="anthropic_api_key=$ANTHROPIC_API_KEY" \
  -var="inngest_event_key=$INNGEST_EVENT_KEY" \
  -var="inngest_signing_key=$INNGEST_SIGNING_KEY" \
  -var="runpod_api_key=$RUNPOD_API_KEY" \
  -var="supabase_url=$SUPABASE_URL" \
  -var="supabase_anon_key=$SUPABASE_ANON_KEY" \
  -var="supabase_service_role_key=$SUPABASE_SERVICE_ROLE_KEY"
```

### Using Just Commands

The Justfile provides convenient commands:

```bash
# Deploy to staging
just deploy-staging

# Deploy to production
just deploy-prod

# Plan changes for development
just infra-plan-dev

# Destroy development environment
just infra-destroy-dev
```

## State Management

For production use, configure remote state backend in `main.tf`:

```hcl
terraform {
  backend "s3" {
    bucket = "framecast-terraform-state"
    key    = "framecast/terraform.tfstate"
    region = "us-east-1"
  }
}
```

## Database Options

The configuration supports two database options:

### Option 1: Supabase (Recommended)

Set these variables:

- `supabase_url`
- `supabase_anon_key`
- `supabase_service_role_key`

Benefits:

- Managed service with built-in auth
- Real-time subscriptions
- Automatic backups
- Built-in REST API

### Option 2: RDS PostgreSQL

If Supabase variables are not set, RDS will be provisioned automatically.

Benefits:

- Full control over database
- VPC isolation
- Custom backup schedules
- Enhanced monitoring

## Security

- S3 buckets have public access blocked
- RDS is in private subnets (when used)
- Lambda functions use least-privilege IAM roles
- Secrets are stored in AWS Secrets Manager
- All resources are tagged for cost tracking

## Monitoring

Production environment includes:

- CloudWatch alarms for Lambda errors
- CloudWatch alarms for API Gateway 5xx errors
- Enhanced RDS monitoring
- Structured logging to CloudWatch Logs

## Cost Optimization

- Lambda functions use ARM64 (graviton2) for better price/performance
- S3 lifecycle policies for output cleanup
- RDS uses smaller instances for non-production
- CloudWatch log retention configured per environment

## Cleanup

To destroy infrastructure:

```bash
# Development
tofu destroy -var-file=environments/dev.tfvars

# Production (be careful!)
tofu destroy -var-file=environments/prod.tfvars
```
