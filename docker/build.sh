#!/bin/bash

# # Step 0: Build frontend with pnpm in ../web
# echo "Step 0: Building frontend in ../web using pnpm..."
# cd ../web || { echo "Failed to enter ../web directory"; exit 1; }
# pnpm run build || { echo "Frontend build failed"; exit 1; }

# # Step 1: Build Rust backend with cargo in project root
# echo "Step 1: Building Rust backend with cargo..."
# cd ../ || { echo "Failed to enter project root directory"; exit 1; }
# cargo build --release || { echo "Rust backend build failed"; exit 1; }

# # Step 2: Back to docker directory
# echo "Step 2: Returning to docker directory..."
# cd docker || { echo "Failed to enter docker directory"; exit 1; }

# # Step 3: Copy the compiled RustMailer binary to current directory (docker/)
# cp ../target/release/rustmailer .
# echo "Step 3: Copied rustmailer binary"

# Step 4: Extract version from Cargo.toml
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' ../Cargo.toml)
echo "Step 4: Extracted version: $VERSION"

DOCKER_API_VERSION=1.44
CI_REGISTRY_IMAGE=registry.kolbein.nl/rustmailer
CI_REGISTRY_USER=kolbeindocker
CI_REGISTRY_TOKEN=IvNorAj4oj
CI_VERSION=$VERSION

# Step 5: Login to Docker registry
docker login $CI_REGISTRY_IMAGE -u "$CI_REGISTRY_USER" -p "$CI_REGISTRY_TOKEN"
echo "Step 5: Logged in to Docker registry"

# Step 6: Pull latest image for caching
docker pull $CI_REGISTRY_IMAGE:latest || true
echo "Step 6: Pulled latest image for caching (if it exists)"

# Step 7: Build Docker image with version tag
# docker build --tag $CI_REGISTRY_IMAGE:$CI_VERSION .
docker build --build-arg CRATE_VERSION=$VERSION --cache-from $CI_REGISTRY_IMAGE:latest --tag $CI_REGISTRY_IMAGE:$CI_VERSION .
echo "Step 7: Built Docker image with tag rustmailer:$VERSION"

# Step 8: Tag and push the image to registry
docker tag $CI_REGISTRY_IMAGE:$CI_VERSION $CI_REGISTRY_IMAGE:latest
echo "Step 8: Tagged image as $CI_REGISTRY_IMAGE:latest"

# Step 9: Push the image to registry
docker push $CI_REGISTRY_IMAGE:$CI_VERSION
echo "Step 9: Pushed image to registry with tag $CI_REGISTRY_IMAGE:$CI_VERSION"
