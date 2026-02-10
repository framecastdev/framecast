# OpenTofu Version Requirements
# Provider versions and required OpenTofu version

terraform {
  required_version = ">= 1.5"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  # Remote state backend (S3 + DynamoDB locking)
  # Configure per-environment via backend config files:
  #   tofu init -backend-config=environments/backend-dev.hcl
  #   tofu init -backend-config=environments/backend-prod.hcl
  # For local development with LocalStack:
  #   tofu init -backend-config=environments/backend-local.hcl
  backend "s3" {}
}
