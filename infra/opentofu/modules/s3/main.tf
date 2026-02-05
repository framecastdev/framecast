# S3 Module - Output and Asset Buckets
# Creates S3 buckets for video outputs and user assets

variable "name_prefix" {
  description = "Prefix for resource names"
  type        = string
}

variable "environment" {
  description = "Environment name"
  type        = string
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}

variable "outputs_expiration_days" {
  description = "Days before output files expire"
  type        = number
  default     = 90
}

variable "versioning_enabled" {
  description = "Enable versioning on buckets"
  type        = bool
  default     = true
}

variable "localstack_enabled" {
  description = "Whether deploying to LocalStack (skips AWS-specific features)"
  type        = bool
  default     = false
}

# Get AWS account ID for unique bucket names (only for real AWS)
data "aws_caller_identity" "current" {
  count = var.localstack_enabled ? 0 : 1
}

locals {
  # For LocalStack, use simple bucket names; for AWS, include account ID for uniqueness
  bucket_suffix = var.localstack_enabled ? "" : "-${data.aws_caller_identity.current[0].account_id}"
}

# Outputs Bucket - for generated videos
resource "aws_s3_bucket" "outputs" {
  bucket = "${var.name_prefix}-outputs${local.bucket_suffix}"

  tags = var.tags
}

resource "aws_s3_bucket_public_access_block" "outputs" {
  bucket = aws_s3_bucket.outputs.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_versioning" "outputs" {
  bucket = aws_s3_bucket.outputs.id
  versioning_configuration {
    status = var.versioning_enabled ? "Enabled" : "Disabled"
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "outputs" {
  count  = var.localstack_enabled ? 0 : 1
  bucket = aws_s3_bucket.outputs.id

  rule {
    id     = "cleanup-old-outputs"
    status = "Enabled"

    filter {
      prefix = ""
    }

    expiration {
      days = var.outputs_expiration_days
    }

    noncurrent_version_expiration {
      noncurrent_days = 30
    }
  }
}

# Assets Bucket - for user uploads
resource "aws_s3_bucket" "assets" {
  bucket = "${var.name_prefix}-assets${local.bucket_suffix}"

  tags = var.tags
}

resource "aws_s3_bucket_public_access_block" "assets" {
  bucket = aws_s3_bucket.assets.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_versioning" "assets" {
  bucket = aws_s3_bucket.assets.id
  versioning_configuration {
    status = var.versioning_enabled ? "Enabled" : "Disabled"
  }
}

resource "aws_s3_bucket_cors_configuration" "assets" {
  bucket = aws_s3_bucket.assets.id

  cors_rule {
    allowed_headers = ["*"]
    allowed_methods = ["GET", "PUT", "POST", "DELETE", "HEAD"]
    allowed_origins = ["*"]
    expose_headers  = ["ETag"]
    max_age_seconds = 3000
  }
}

# Outputs
output "outputs_bucket_name" {
  description = "Outputs S3 bucket name"
  value       = aws_s3_bucket.outputs.bucket
}

output "outputs_bucket_arn" {
  description = "Outputs S3 bucket ARN"
  value       = aws_s3_bucket.outputs.arn
}

output "assets_bucket_name" {
  description = "Assets S3 bucket name"
  value       = aws_s3_bucket.assets.bucket
}

output "assets_bucket_arn" {
  description = "Assets S3 bucket ARN"
  value       = aws_s3_bucket.assets.arn
}
