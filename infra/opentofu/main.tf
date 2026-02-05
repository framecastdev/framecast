# Framecast Infrastructure - OpenTofu Configuration
# Deploys Lambda, API Gateway, S3 buckets, and monitoring
#
# Usage:
#   AWS:        tofu apply -var-file=environments/dev.tfvars
#   LocalStack: tofu apply -var-file=environments/localstack.tfvars

# ==============================================================================
# PROVIDERS
# ==============================================================================

provider "aws" {
  region = var.aws_region

  # LocalStack configuration
  skip_credentials_validation = var.localstack_enabled
  skip_metadata_api_check     = var.localstack_enabled
  skip_requesting_account_id  = var.localstack_enabled

  dynamic "endpoints" {
    for_each = var.localstack_enabled ? [1] : []
    content {
      apigateway     = var.localstack_endpoint
      apigatewayv2   = var.localstack_endpoint
      cloudwatch     = var.localstack_endpoint
      cloudwatchlogs = var.localstack_endpoint
      iam            = var.localstack_endpoint
      lambda         = var.localstack_endpoint
      s3             = var.localstack_endpoint
      sts            = var.localstack_endpoint
    }
  }

  default_tags {
    tags = {
      Project     = "Framecast"
      Environment = var.environment
      ManagedBy   = "OpenTofu"
    }
  }
}

# ==============================================================================
# LOCALS
# ==============================================================================

locals {
  name_prefix = "framecast-${var.environment}"

  common_tags = {
    Project     = "Framecast"
    Environment = var.environment
    ManagedBy   = "OpenTofu"
  }

  # Enable monitoring for production by default
  enable_monitoring = var.enable_monitoring != null ? var.enable_monitoring : (var.environment == "prod")

  # Lambda environment variables
  lambda_environment = {
    DATABASE_URL        = var.database_url
    JWT_SECRET          = var.jwt_secret
    SUPABASE_URL        = var.supabase_url
    SUPABASE_ANON_KEY   = var.supabase_anon_key
    ANTHROPIC_API_KEY   = var.anthropic_api_key
    INNGEST_EVENT_KEY   = var.inngest_event_key
    INNGEST_SIGNING_KEY = var.inngest_signing_key
    RUNPOD_API_KEY      = var.runpod_api_key
    RUNPOD_ENDPOINT_ID  = var.runpod_endpoint_id
    S3_BUCKET_OUTPUTS   = module.s3.outputs_bucket_name
    S3_BUCKET_ASSETS    = module.s3.assets_bucket_name
    # For LocalStack, set the endpoint URL
    AWS_ENDPOINT_URL = var.localstack_enabled ? var.localstack_endpoint : ""
  }
}

# ==============================================================================
# S3 BUCKETS
# ==============================================================================

module "s3" {
  source = "./modules/s3"

  name_prefix             = local.name_prefix
  environment             = var.environment
  outputs_expiration_days = var.outputs_expiration_days
  tags                    = local.common_tags

  # LocalStack configuration
  localstack_enabled = var.localstack_enabled

  # Disable versioning for LocalStack (not fully supported)
  versioning_enabled = !var.localstack_enabled
}

# ==============================================================================
# LAMBDA FUNCTION
# ==============================================================================

module "lambda" {
  source = "./modules/lambda"

  name_prefix           = local.name_prefix
  environment           = var.environment
  lambda_zip_path       = var.lambda_zip_path
  memory_size           = var.lambda_memory_size
  timeout               = var.lambda_timeout
  environment_variables = local.lambda_environment
  tags                  = local.common_tags

  # Log retention
  log_retention_days = var.environment == "prod" ? 30 : 14

  # S3 bucket access
  s3_bucket_arns = [
    module.s3.outputs_bucket_arn,
    module.s3.assets_bucket_arn
  ]
}

# ==============================================================================
# API GATEWAY
# ==============================================================================

module "api_gateway" {
  source = "./modules/api-gateway"

  name_prefix          = local.name_prefix
  environment          = var.environment
  lambda_invoke_arn    = module.lambda.invoke_arn
  lambda_function_name = module.lambda.function_name
  tags                 = local.common_tags
}

# ==============================================================================
# MONITORING (Production only by default)
# ==============================================================================

module "monitoring" {
  source = "./modules/monitoring"

  name_prefix          = local.name_prefix
  environment          = var.environment
  lambda_function_name = module.lambda.function_name
  api_gateway_id       = module.api_gateway.api_id
  tags                 = local.common_tags

  # Only enable monitoring in production (or if explicitly enabled)
  enabled = local.enable_monitoring && !var.localstack_enabled

  # SNS topic for alarms (if provided)
  alarm_actions = var.alarm_sns_topic_arn != "" ? [var.alarm_sns_topic_arn] : []
}
