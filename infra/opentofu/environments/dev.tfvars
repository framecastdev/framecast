# Development Environment Configuration
# Secrets should be set via TF_VAR_* environment variables or CI/CD

environment     = "dev"
aws_region      = "us-east-1"
lambda_zip_path = "../../target/lambda/lambda/bootstrap.zip"

# Lambda configuration
lambda_memory_size = 512
lambda_timeout     = 30

# S3 configuration
outputs_expiration_days = 30

# CORS (dev frontend)
cors_allow_origins = ["http://localhost:3000"]

# Monitoring (disabled for dev by default)
enable_monitoring = false

# ==============================================================================
# SECRETS - Set these via environment variables:
# ==============================================================================
# export TF_VAR_database_url="postgresql://..."
# export TF_VAR_jwt_secret="..."
# export TF_VAR_anthropic_api_key="..."
