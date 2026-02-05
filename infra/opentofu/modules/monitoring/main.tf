# Monitoring Module - CloudWatch Alarms (Production Only)
# Creates alerts for Lambda errors and API Gateway 5xx errors

variable "name_prefix" {
  description = "Prefix for resource names"
  type        = string
}

variable "environment" {
  description = "Environment name"
  type        = string
}

variable "lambda_function_name" {
  description = "Lambda function name to monitor"
  type        = string
}

variable "api_gateway_id" {
  description = "API Gateway ID to monitor"
  type        = string
}

variable "tags" {
  description = "Tags to apply to resources"
  type        = map(string)
  default     = {}
}

variable "enabled" {
  description = "Whether to create monitoring resources"
  type        = bool
  default     = true
}

variable "lambda_error_threshold" {
  description = "Threshold for Lambda error alarm"
  type        = number
  default     = 10
}

variable "api_5xx_threshold" {
  description = "Threshold for API Gateway 5xx alarm"
  type        = number
  default     = 10
}

variable "alarm_actions" {
  description = "ARNs of SNS topics or other actions for alarms"
  type        = list(string)
  default     = []
}

# Lambda Errors Alarm
resource "aws_cloudwatch_metric_alarm" "lambda_errors" {
  count = var.enabled ? 1 : 0

  alarm_name          = "${var.name_prefix}-lambda-errors"
  alarm_description   = "Alarm for Lambda function errors in ${var.environment}"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "Errors"
  namespace           = "AWS/Lambda"
  period              = 300
  statistic           = "Sum"
  threshold           = var.lambda_error_threshold

  dimensions = {
    FunctionName = var.lambda_function_name
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.alarm_actions

  tags = var.tags
}

# API Gateway 5xx Errors Alarm
resource "aws_cloudwatch_metric_alarm" "api_5xx" {
  count = var.enabled ? 1 : 0

  alarm_name          = "${var.name_prefix}-api-5xx-errors"
  alarm_description   = "Alarm for API Gateway 5xx errors in ${var.environment}"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "5XXError"
  namespace           = "AWS/ApiGateway"
  period              = 300
  statistic           = "Sum"
  threshold           = var.api_5xx_threshold

  dimensions = {
    ApiId = var.api_gateway_id
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.alarm_actions

  tags = var.tags
}

# Lambda Duration Alarm (for detecting performance issues)
resource "aws_cloudwatch_metric_alarm" "lambda_duration" {
  count = var.enabled ? 1 : 0

  alarm_name          = "${var.name_prefix}-lambda-duration"
  alarm_description   = "Alarm for Lambda function duration in ${var.environment}"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 3
  metric_name         = "Duration"
  namespace           = "AWS/Lambda"
  period              = 300
  statistic           = "Average"
  threshold           = 5000 # 5 seconds

  dimensions = {
    FunctionName = var.lambda_function_name
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.alarm_actions

  tags = var.tags
}

# Lambda Throttles Alarm
resource "aws_cloudwatch_metric_alarm" "lambda_throttles" {
  count = var.enabled ? 1 : 0

  alarm_name          = "${var.name_prefix}-lambda-throttles"
  alarm_description   = "Alarm for Lambda throttling in ${var.environment}"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "Throttles"
  namespace           = "AWS/Lambda"
  period              = 60
  statistic           = "Sum"
  threshold           = 1

  dimensions = {
    FunctionName = var.lambda_function_name
  }

  alarm_actions = var.alarm_actions
  ok_actions    = var.alarm_actions

  tags = var.tags
}

# Outputs
output "lambda_errors_alarm_arn" {
  description = "Lambda errors alarm ARN"
  value       = var.enabled ? aws_cloudwatch_metric_alarm.lambda_errors[0].arn : null
}

output "api_5xx_alarm_arn" {
  description = "API Gateway 5xx alarm ARN"
  value       = var.enabled ? aws_cloudwatch_metric_alarm.api_5xx[0].arn : null
}
