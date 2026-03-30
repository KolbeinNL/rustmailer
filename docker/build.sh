#!/bin/bash

read -p "Build first? y/n " build

if [[ "$build" == "y" ]]; then
    echo "Building the project before creating the Docker image..."
    # Step 0: Build frontend with pnpm in ../web
    echo "Step 0: Building frontend in ../web using pnpm..."
    cd ../web || { echo "Failed to enter ../web directory"; exit 1; }
    pnpm run build || { echo "Frontend build failed"; exit 1; }

    # Step 1: Build Rust backend with cargo in project root
    echo "Step 1: Building Rust backend with cargo..."
    cd ../ || { echo "Failed to enter project root directory"; exit 1; }
    cargo build --release || { echo "Rust backend build failed"; exit 1; }

    # Step 2: Back to docker directory
    echo "Step 2: Returning to docker directory..."
    cd docker || { echo "Failed to enter docker directory"; exit 1; }

    # Step 3: Copy the compiled RustMailer binary to current directory (docker/)
    echo "Step 3: Copying rustmailer binary"
    cp ../target/release/rustmailer .
else
    echo "Skipping building. Proceeding to create Docker image from step 4..."
fi

# Step 4: Extract version from Cargo.toml
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' ../Cargo.toml)
echo "Step 4: Extracted version: $VERSION"

DOCKER_API_VERSION=1.44
CI_REGISTRY_IMAGE=registry.kolbein.nl/rustmailer
CI_REGISTRY_USER=kolbeindocker
CI_REGISTRY_TOKEN=IvNorAj4oj
CI_VERSION=$VERSION

# Step 5: Login to Docker registry
echo "Step 5: Logging in to Docker registry"
docker login $CI_REGISTRY_IMAGE -u "$CI_REGISTRY_USER" -p "$CI_REGISTRY_TOKEN"

# Step 6: Pull latest image for caching
echo "Step 6: Pulling latest image for caching (if it exists)"
docker pull $CI_REGISTRY_IMAGE:latest || true

# Step 7: Build Docker image with version tag
# docker build --tag $CI_REGISTRY_IMAGE:$CI_VERSION .
echo "Step 7: Building Docker image with tag rustmailer:$VERSION"
docker build --build-arg CRATE_VERSION=$VERSION --cache-from $CI_REGISTRY_IMAGE:latest --tag $CI_REGISTRY_IMAGE:$CI_VERSION .

# Step 8: Push the image to registry
echo "Step 8: Pushing image to registry with tag $CI_REGISTRY_IMAGE:$CI_VERSION"
docker push $CI_REGISTRY_IMAGE:$CI_VERSION

# Step 9: Pull latest image for caching
echo "Step 9: Pulling just released image for caching"
docker pull $CI_REGISTRY_IMAGE:$CI_VERSION

# Step 10: Tag and push the image to registry
echo "Step 10: Tagging image as $CI_REGISTRY_IMAGE:latest"
docker tag $CI_REGISTRY_IMAGE:$CI_VERSION $CI_REGISTRY_IMAGE:latest

# Step 11: Push the image to registry
echo "Step 11: Pushing image to registry with tag $CI_REGISTRY_IMAGE:latest"
docker push $CI_REGISTRY_IMAGE:latest
