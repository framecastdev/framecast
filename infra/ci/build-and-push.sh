#!/bin/bash
set -e

REGISTRY="192.168.68.77:3000"
IMAGE_NAME="thiago/framecast-ci"
TAG="${1:-latest}"

echo "Building CI image: ${REGISTRY}/${IMAGE_NAME}:${TAG}"
docker build -t ${REGISTRY}/${IMAGE_NAME}:${TAG} -f infra/ci/Dockerfile .

echo "Pushing to registry..."
docker push ${REGISTRY}/${IMAGE_NAME}:${TAG}

echo "Done! Image: ${REGISTRY}/${IMAGE_NAME}:${TAG}"
