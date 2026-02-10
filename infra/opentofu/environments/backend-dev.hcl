# Backend configuration for dev environment
bucket         = "framecast-terraform-state"
key            = "dev/terraform.tfstate"
region         = "us-east-1"
dynamodb_table = "framecast-terraform-locks"
encrypt        = true
