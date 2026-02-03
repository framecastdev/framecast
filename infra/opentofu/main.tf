# Framecast Infrastructure - OpenTofu Configuration
# Based on 12-Factor principles with backing services

terraform {
  required_version = ">= 1.5"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    archive = {
      source  = "hashicorp/archive"
      version = "~> 2.4"
    }
  }

  # Backend configuration (configure per environment)
  # backend "s3" {
  #   bucket = "framecast-terraform-state"
  #   key    = "framecast/terraform.tfstate"
  #   region = "us-east-1"
  # }
}

# Provider configuration
provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "Framecast"
      Environment = var.environment
      ManagedBy   = "OpenTofu"
    }
  }
}

# Data sources
data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

# ============================================================================
# VARIABLES
# ============================================================================

variable "environment" {
  description = "Environment name (dev, staging, prod)"
  type        = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be one of: dev, staging, prod"
  }
}

variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "domain_name" {
  description = "Domain name for the API (optional)"
  type        = string
  default     = null
}

variable "supabase_url" {
  description = "Supabase project URL (if using Supabase instead of RDS)"
  type        = string
  default     = null
  sensitive   = true
}

variable "supabase_anon_key" {
  description = "Supabase anonymous key"
  type        = string
  default     = null
  sensitive   = true
}

variable "supabase_service_role_key" {
  description = "Supabase service role key"
  type        = string
  default     = null
  sensitive   = true
}

# NOTE: Lambda environment variables (API keys, etc.) are now configured
# in template.yaml SAM parameters. Only database-related variables remain here.

# ============================================================================
# LOCAL VALUES
# ============================================================================

locals {
  name_prefix = "framecast-${var.environment}"

  common_tags = {
    Project     = "Framecast"
    Environment = var.environment
    ManagedBy   = "OpenTofu"
  }

  # NOTE: Lambda and API Gateway are now managed by AWS SAM (template.yaml)
  # This file only manages non-Lambda infrastructure (RDS, VPC, security groups)

  # Database configuration (use RDS if Supabase URL not provided)
  use_rds = var.supabase_url == null
}

# ============================================================================
# S3 BUCKETS - Now managed by AWS SAM (template.yaml)
# ============================================================================
# NOTE: S3 buckets for outputs and assets are defined in template.yaml
# to keep all Lambda-related resources together for atomic deployments.

# ============================================================================
# RDS POSTGRESQL - Database (12-Factor Rule IV)
# ============================================================================

# RDS subnet group
resource "aws_db_subnet_group" "main" {
  count = local.use_rds ? 1 : 0

  name       = "${local.name_prefix}-db-subnet-group"
  subnet_ids = data.aws_subnets.default.ids
  tags       = local.common_tags
}

# RDS parameter group
resource "aws_db_parameter_group" "main" {
  count = local.use_rds ? 1 : 0

  family = "postgres15"
  name   = "${local.name_prefix}-db-params"

  parameter {
    name  = "shared_preload_libraries"
    value = "pg_stat_statements"
  }

  tags = local.common_tags
}

# RDS instance
resource "aws_db_instance" "main" {
  count = local.use_rds ? 1 : 0

  identifier = "${local.name_prefix}-db"

  # Engine configuration
  engine         = "postgres"
  engine_version = "15.4"
  instance_class = var.environment == "prod" ? "db.t3.small" : "db.t3.micro"

  # Storage configuration
  allocated_storage     = var.environment == "prod" ? 100 : 20
  max_allocated_storage = var.environment == "prod" ? 500 : 100
  storage_type          = "gp2"
  storage_encrypted     = true

  # Database configuration
  db_name  = "framecast"
  username = "postgres"
  password = random_password.db_password[0].result

  # Networking
  db_subnet_group_name   = aws_db_subnet_group.main[0].name
  vpc_security_group_ids = [aws_security_group.rds[0].id]
  publicly_accessible    = false

  # Backup configuration
  backup_retention_period = var.environment == "prod" ? 30 : 7
  backup_window           = "03:00-04:00"
  maintenance_window      = "sun:04:00-sun:05:00"

  # Monitoring
  monitoring_interval = var.environment == "prod" ? 60 : 0
  monitoring_role_arn = var.environment == "prod" ? aws_iam_role.rds_monitoring[0].arn : null

  # Parameter group
  parameter_group_name = aws_db_parameter_group.main[0].name

  # Deletion protection for production
  deletion_protection = var.environment == "prod"
  skip_final_snapshot = var.environment != "prod"

  tags = local.common_tags
}

# Random password for RDS
resource "random_password" "db_password" {
  count = local.use_rds ? 1 : 0

  length  = 32
  special = true
}

# Store RDS password in Secrets Manager
resource "aws_secretsmanager_secret" "db_password" {
  count = local.use_rds ? 1 : 0

  name        = "${local.name_prefix}/rds/password"
  description = "RDS password for Framecast database"
  tags        = local.common_tags
}

resource "aws_secretsmanager_secret_version" "db_password" {
  count = local.use_rds ? 1 : 0

  secret_id     = aws_secretsmanager_secret.db_password[0].id
  secret_string = random_password.db_password[0].result
}

# ============================================================================
# NETWORKING - Default VPC and Security Groups
# ============================================================================

# Get default VPC and subnets
data "aws_vpc" "default" {
  default = true
}

data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default.id]
  }
}

# Security group for RDS
# NOTE: Lambda is now managed by SAM. If using VPC-enabled Lambda,
# configure the Lambda security group in template.yaml and reference it here.
resource "aws_security_group" "rds" {
  count = local.use_rds ? 1 : 0

  name_prefix = "${local.name_prefix}-rds-"
  vpc_id      = data.aws_vpc.default.id
  description = "Security group for RDS database"

  # Allow PostgreSQL from within the VPC (Lambda will need VPC access configured in SAM)
  ingress {
    description = "PostgreSQL from VPC"
    from_port   = 5432
    to_port     = 5432
    protocol    = "tcp"
    cidr_blocks = [data.aws_vpc.default.cidr_block]
  }

  egress {
    description = "All outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = local.common_tags

  lifecycle {
    create_before_destroy = true
  }
}

# RDS monitoring role
resource "aws_iam_role" "rds_monitoring" {
  count = local.use_rds && var.environment == "prod" ? 1 : 0

  name = "${local.name_prefix}-rds-monitoring"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "monitoring.rds.amazonaws.com"
        }
      }
    ]
  })

  tags = local.common_tags
}

resource "aws_iam_role_policy_attachment" "rds_monitoring" {
  count = local.use_rds && var.environment == "prod" ? 1 : 0

  role       = aws_iam_role.rds_monitoring[0].name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonRDSEnhancedMonitoringRole"
}

# ============================================================================
# OUTPUTS
# ============================================================================

# NOTE: S3 bucket outputs are now provided by SAM stack (template.yaml)
# Use `just sam-outputs` to see Lambda/S3 related outputs

output "database_endpoint" {
  description = "RDS instance endpoint"
  value       = local.use_rds ? aws_db_instance.main[0].endpoint : null
  sensitive   = true
}

output "database_url" {
  description = "Database connection URL"
  value       = local.use_rds ? "postgresql://postgres:${random_password.db_password[0].result}@${aws_db_instance.main[0].endpoint}:5432/framecast" : var.supabase_url
  sensitive   = true
}

output "rds_security_group_id" {
  description = "RDS security group ID (for VPC Lambda configuration)"
  value       = local.use_rds ? aws_security_group.rds[0].id : null
  sensitive   = true
}

output "vpc_id" {
  description = "VPC ID"
  value       = data.aws_vpc.default.id
}

output "subnet_ids" {
  description = "Subnet IDs for VPC Lambda"
  value       = data.aws_subnets.default.ids
}

output "aws_region" {
  description = "AWS region"
  value       = data.aws_region.current.name
}

output "environment" {
  description = "Environment name"
  value       = var.environment
}
