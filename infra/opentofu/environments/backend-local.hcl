# Backend configuration for local development (LocalStack)
bucket                      = "framecast-terraform-state"
key                         = "local/terraform.tfstate"
region                      = "us-east-1"
endpoint                    = "http://localhost:4566"
dynamodb_table              = "framecast-terraform-locks"
dynamodb_endpoint           = "http://localhost:4566"
encrypt                     = false
skip_credentials_validation = true
skip_metadata_api_check     = true
force_path_style            = true
