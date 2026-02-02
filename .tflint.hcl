# TFLint configuration for OpenTofu/Terraform
# Linting rules for infrastructure code quality

config {
  # Enable all rules by default
  call_module_type = "all"
  force = false
  disabled_by_default = false
}

# AWS plugin for AWS-specific rules
plugin "aws" {
  enabled = true
  version = "0.29.0"
  source  = "github.com/terraform-linters/tflint-ruleset-aws"

  # Deep check requires AWS credentials
  deep_check = false
}

# Core rules
rule "terraform_deprecated_interpolation" {
  enabled = true
}

rule "terraform_deprecated_index" {
  enabled = true
}

rule "terraform_unused_declarations" {
  enabled = true
}

rule "terraform_comment_syntax" {
  enabled = true
}

rule "terraform_documented_outputs" {
  enabled = true
}

rule "terraform_documented_variables" {
  enabled = true
}

rule "terraform_typed_variables" {
  enabled = true
}

rule "terraform_module_pinned_source" {
  enabled = true
}

rule "terraform_naming_convention" {
  enabled = true
  format  = "snake_case"
}

rule "terraform_standard_module_structure" {
  enabled = true
}

# AWS-specific rules
rule "aws_instance_invalid_type" {
  enabled = true
}

rule "aws_instance_previous_type" {
  enabled = true
}

rule "aws_route_not_specified_target" {
  enabled = true
}

rule "aws_route_specified_multiple_targets" {
  enabled = true
}

rule "aws_security_group_rule_invalid_protocol" {
  enabled = true
}

rule "aws_s3_bucket_invalid_acl" {
  enabled = true
}

rule "aws_s3_bucket_invalid_storage_class" {
  enabled = true
}

# Security rules
rule "aws_security_group_rule_invalid_protocol" {
  enabled = true
}

rule "aws_db_instance_readable_password" {
  enabled = true
}

rule "aws_elasticache_cluster_readable_password" {
  enabled = true
}

rule "aws_iam_policy_document_gov_friendly_arns" {
  enabled = true
}

rule "aws_iam_role_policy_gov_friendly_arns" {
  enabled = true
}

rule "aws_iam_user_policy_gov_friendly_arns" {
  enabled = true
}