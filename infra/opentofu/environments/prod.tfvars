# Production Environment Configuration
# Secrets should be set via TF_VAR_* environment variables or CI/CD

environment     = "prod"
aws_region      = "us-east-1"
lambda_zip_path = "../../target/lambda/lambda/bootstrap.zip"

# Lambda configuration (production tuned)
lambda_memory_size = 1024
lambda_timeout     = 30

# S3 configuration
outputs_expiration_days = 90

# Monitoring (always enabled for production)
enable_monitoring = true

# CORS (restrict to actual domain in production)
cors_allow_origins = ["https://framecast.app"]

# ==============================================================================
# SECRETS - Set these via environment variables:
# ==============================================================================
# export TF_VAR_database_url="postgresql://..."
# export TF_VAR_jwt_secret="..."
# export TF_VAR_anthropic_api_key="..."
#
# Optional: SNS topic for alarm notifications
# export TF_VAR_alarm_sns_topic_arn="arn:aws:sns:..."
