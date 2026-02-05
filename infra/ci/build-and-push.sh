#!/bin/bash
set -e

REGISTRY="ghcr.io"
IMAGE_NAME="framecastdev/framecast-ci"
TAG="${1:-latest}"

echo "Building CI image: ${REGISTRY}/${IMAGE_NAME}:${TAG}"
docker build -t ${REGISTRY}/${IMAGE_NAME}:${TAG} -f infra/ci/Dockerfile .

echo "Pushing to registry..."
docker push ${REGISTRY}/${IMAGE_NAME}:${TAG}

echo "Done! Image: ${REGISTRY}/${IMAGE_NAME}:${TAG}"
