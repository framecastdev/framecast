# Lambda Module - Framecast API Function
# Creates Lambda function, IAM role, and CloudWatch Log Group

variable "name_prefix" {
  description = "Prefix for resource names"
  type        = string
}

variable "environment" {
  description = "Environment name"
  type        = string
}

variable "lambda_zip_path" {
  description = "Path to the Lambda deployment ZIP file"
  type        = string
}

variable "memory_size" {
  description = "Lambda memory size in MB"
  type        = number
  default     = 512
}

variable "timeout" {
  description = "Lambda timeout in seconds"
  type        = number
  default     = 30
}

variable "environment_variables" {
  description = "Environment variables for the Lambda function"
  type        = map(string)
  default     = {}
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}

variable "log_retention_days" {
  description = "CloudWatch log retention in days"
  type        = number
  default     = 14
}

variable "s3_bucket_arns" {
  description = "S3 bucket ARNs for IAM policy"
  type        = list(string)
  default     = []
}

# IAM Role for Lambda
resource "aws_iam_role" "lambda" {
  name = "${var.name_prefix}-lambda-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "lambda.amazonaws.com"
        }
      }
    ]
  })

  tags = var.tags
}

# CloudWatch Logs policy
resource "aws_iam_role_policy" "lambda_logs" {
  name = "${var.name_prefix}-lambda-logs"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "logs:CreateLogGroup",
          "logs:CreateLogStream",
          "logs:PutLogEvents"
        ]
        Resource = [
          "${aws_cloudwatch_log_group.lambda.arn}",
          "${aws_cloudwatch_log_group.lambda.arn}:*"
        ]
      }
    ]
  })
}

# S3 access policy (if buckets are provided)
resource "aws_iam_role_policy" "lambda_s3" {
  count = length(var.s3_bucket_arns) > 0 ? 1 : 0

  name = "${var.name_prefix}-lambda-s3"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:PutObject",
          "s3:DeleteObject",
          "s3:ListBucket"
        ]
        Resource = concat(
          var.s3_bucket_arns,
          [for arn in var.s3_bucket_arns : "${arn}/*"]
        )
      }
    ]
  })
}

# CloudWatch Log Group
resource "aws_cloudwatch_log_group" "lambda" {
  name              = "/aws/lambda/${var.name_prefix}-api"
  retention_in_days = var.log_retention_days

  tags = var.tags
}

# Lambda Function
resource "aws_lambda_function" "api" {
  function_name = "${var.name_prefix}-api"
  description   = "Framecast API - ${var.environment}"

  filename         = var.lambda_zip_path
  source_code_hash = filebase64sha256(var.lambda_zip_path)
  handler          = "bootstrap"
  runtime          = "provided.al2023"
  architectures    = ["x86_64"]

  role        = aws_iam_role.lambda.arn
  memory_size = var.memory_size
  timeout     = var.timeout

  environment {
    variables = merge(
      {
        ENVIRONMENT = var.environment
        RUST_LOG    = var.environment == "prod" ? "info" : "debug,sqlx=warn"
      },
      var.environment_variables
    )
  }

  depends_on = [
    aws_cloudwatch_log_group.lambda,
    aws_iam_role_policy.lambda_logs
  ]

  tags = var.tags
}

# Outputs
output "function_name" {
  description = "Lambda function name"
  value       = aws_lambda_function.api.function_name
}

output "function_arn" {
  description = "Lambda function ARN"
  value       = aws_lambda_function.api.arn
}

output "invoke_arn" {
  description = "Lambda invoke ARN for API Gateway"
  value       = aws_lambda_function.api.invoke_arn
}

output "role_arn" {
  description = "Lambda execution role ARN"
  value       = aws_iam_role.lambda.arn
}

output "log_group_name" {
  description = "CloudWatch Log Group name"
  value       = aws_cloudwatch_log_group.lambda.name
}
