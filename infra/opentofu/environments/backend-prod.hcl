# Backend configuration for production environment
bucket         = "framecast-terraform-state"
key            = "prod/terraform.tfstate"
region         = "us-east-1"
dynamodb_table = "framecast-terraform-locks"
encrypt        = true
