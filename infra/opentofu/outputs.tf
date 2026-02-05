# Outputs for Framecast Infrastructure
# These values are used by CI/CD and other automation

output "api_endpoint" {
  description = "API Gateway endpoint URL"
  value       = module.api_gateway.api_endpoint
}

output "lambda_function_name" {
  description = "Lambda function name"
  value       = module.lambda.function_name
}

output "lambda_function_arn" {
  description = "Lambda function ARN"
  value       = module.lambda.function_arn
}

output "outputs_bucket_name" {
  description = "S3 bucket for video outputs"
  value       = module.s3.outputs_bucket_name
}

output "assets_bucket_name" {
  description = "S3 bucket for user assets"
  value       = module.s3.assets_bucket_name
}

output "log_group_name" {
  description = "CloudWatch Log Group for Lambda"
  value       = module.lambda.log_group_name
}

output "environment" {
  description = "Environment name"
  value       = var.environment
}

output "aws_region" {
  description = "AWS region"
  value       = var.aws_region
}
