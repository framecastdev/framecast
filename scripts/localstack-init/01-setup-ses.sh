#!/bin/bash

# LocalStack SES Setup Script
# Configures SES service for email testing

set -e

echo "🚀 Setting up LocalStack SES service..."

# Wait for LocalStack to be ready
while ! curl -s http://localhost:4566/_localstack/health | grep -q -E '"ses": "(available|running)"'; do
    echo "⏳ Waiting for LocalStack SES service..."
    sleep 2
done

echo "✅ LocalStack SES service is ready"

# Configure AWS CLI for LocalStack
export AWS_ACCESS_KEY_ID=test-access-key
export AWS_SECRET_ACCESS_KEY=test-secret-key
export AWS_DEFAULT_REGION=us-east-1
export AWS_ENDPOINT_URL=http://localhost:4566

echo "📧 Configuring SES identities..."

# Verify email addresses for testing
EMAIL_ADDRESSES=(
    "invitations@framecast.app"
    "noreply@framecast.app"
    "support@framecast.app"
    "test@framecast.app"
    "invitee@example.com"
    "developer@example.com"
    "admin@example.com"
    "user@test.com"
)

for email in "${EMAIL_ADDRESSES[@]}"; do
    echo "✉️ Verifying email identity: $email"
    aws ses verify-email-identity \
        --email-address "$email" \
        --endpoint-url "$AWS_ENDPOINT_URL" \
        --region "$AWS_DEFAULT_REGION" || echo "Failed to verify $email"
done

echo "🔍 Listing verified identities..."
aws ses list-identities \
    --endpoint-url "$AWS_ENDPOINT_URL" \
    --region "$AWS_DEFAULT_REGION"

echo "📊 Getting SES send quota..."
aws ses get-send-quota \
    --endpoint-url "$AWS_ENDPOINT_URL" \
    --region "$AWS_DEFAULT_REGION"

echo "✅ SES setup completed successfully!"

# Create S3 buckets for testing if needed
echo "🪣 Creating S3 buckets for testing..."
BUCKET_NAMES=(
    "framecast-emails-test"
    "framecast-assets-test"
    "framecast-outputs-test"
)

for bucket in "${BUCKET_NAMES[@]}"; do
    echo "📁 Creating bucket: $bucket"
    aws s3 mb "s3://$bucket" \
        --endpoint-url "$AWS_ENDPOINT_URL" \
        --region "$AWS_DEFAULT_REGION" || echo "Bucket $bucket may already exist"
done

echo "🎉 LocalStack initialization completed!"