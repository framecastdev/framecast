# LocalStack Environment Configuration
# For local development and testing

environment     = "dev"
aws_region      = "us-east-1"
lambda_zip_path = "../../../target/lambda/lambda/bootstrap.zip"

# LocalStack settings
localstack_enabled  = true
localstack_endpoint = "http://localhost:4566"

# Lambda configuration
lambda_memory_size = 512
lambda_timeout     = 30

# S3 configuration
outputs_expiration_days = 30

# Monitoring disabled for LocalStack
enable_monitoring = false

# ==============================================================================
# LOCAL DEVELOPMENT SECRETS
# ==============================================================================
# These can be dummy values for local testing

# Database - use local PostgreSQL
# TF_VAR_database_url="postgresql://postgres:postgres@localhost:5432/framecast_dev"

# JWT secret - use a test value
# TF_VAR_jwt_secret="local-dev-jwt-secret-do-not-use-in-production"

# Optional services can be empty for local testing
# TF_VAR_supabase_url=""
# TF_VAR_supabase_anon_key=""
# TF_VAR_anthropic_api_key=""
# TF_VAR_inngest_event_key=""
# TF_VAR_inngest_signing_key=""
# TF_VAR_runpod_api_key=""
# TF_VAR_runpod_endpoint_id=""
