# Backend configuration for staging environment
bucket         = "framecast-terraform-state"
key            = "staging/terraform.tfstate"
region         = "us-east-1"
dynamodb_table = "framecast-terraform-locks"
encrypt        = true
