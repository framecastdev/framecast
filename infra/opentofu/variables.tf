# Input Variables for Framecast Infrastructure
# Configure via .tfvars files or TF_VAR_* environment variables

# ==============================================================================
# REQUIRED VARIABLES
# ==============================================================================

variable "environment" {
  description = "Environment name (dev, staging, prod)"
  type        = string

  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be one of: dev, staging, prod"
  }
}

variable "lambda_zip_path" {
  description = "Path to the Lambda deployment ZIP file (from cargo-lambda build)"
  type        = string
}

# ==============================================================================
# AWS CONFIGURATION
# ==============================================================================

variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

# ==============================================================================
# LAMBDA ENVIRONMENT VARIABLES (SECRETS)
# ==============================================================================

variable "database_url" {
  description = "PostgreSQL connection URL"
  type        = string
  sensitive   = true
}

variable "jwt_secret" {
  description = "JWT signing secret"
  type        = string
  sensitive   = true
}

variable "supabase_url" {
  description = "Supabase project URL (optional)"
  type        = string
  default     = ""
  sensitive   = true
}

variable "supabase_anon_key" {
  description = "Supabase anonymous key (optional)"
  type        = string
  default     = ""
  sensitive   = true
}

variable "anthropic_api_key" {
  description = "Anthropic API key for LLM (optional)"
  type        = string
  default     = ""
  sensitive   = true
}

variable "inngest_event_key" {
  description = "Inngest event key (optional)"
  type        = string
  default     = ""
  sensitive   = true
}

variable "inngest_signing_key" {
  description = "Inngest signing key (optional)"
  type        = string
  default     = ""
  sensitive   = true
}

variable "runpod_api_key" {
  description = "RunPod API key (optional)"
  type        = string
  default     = ""
  sensitive   = true
}

variable "runpod_endpoint_id" {
  description = "RunPod endpoint ID (optional)"
  type        = string
  default     = ""
}

# ==============================================================================
# LAMBDA CONFIGURATION
# ==============================================================================

variable "lambda_memory_size" {
  description = "Lambda memory size in MB"
  type        = number
  default     = 512
}

variable "lambda_timeout" {
  description = "Lambda timeout in seconds"
  type        = number
  default     = 30
}

# ==============================================================================
# MONITORING
# ==============================================================================

variable "enable_monitoring" {
  description = "Enable CloudWatch alarms (auto-enabled for prod)"
  type        = bool
  default     = null # Will default to true for prod
}

variable "alarm_sns_topic_arn" {
  description = "SNS topic ARN for alarm notifications (optional)"
  type        = string
  default     = ""
}

# ==============================================================================
# S3 CONFIGURATION
# ==============================================================================

variable "outputs_expiration_days" {
  description = "Days before output files expire"
  type        = number
  default     = 90
}

# ==============================================================================
# LOCALSTACK (for local development)
# ==============================================================================

variable "localstack_enabled" {
  description = "Deploy to LocalStack instead of AWS"
  type        = bool
  default     = false
}

variable "localstack_endpoint" {
  description = "LocalStack endpoint URL"
  type        = string
  default     = "http://localhost:4566"
}
