#!/usr/bin/env bash
set -e

pushd .

PROJECT_ROOT=$(dirname "$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")")
# Change to the project root and supports calls from symlinks
cd $PROJECT_ROOT

# Find the current version from Cargo.toml
VERSION=`grep "^version" $PROJECT_ROOT/substrate/bin/node/cli/Cargo.toml | egrep -o "([0-9\.]+)"`
GITUSER=parity
GITREPO=substrate

# Build the image
echo "Building ${GITUSER}/${GITREPO}:latest docker image, hang on!"
# improve the docker logs to actually allow debugging with BuildKit enabled since build time may take an hour
export BUILDKIT_PROGRESS=plain
export DOCKER_BUILDKIT=1
time docker build --no-cache -f $PROJECT_ROOT/substrate/docker/substrate_builder.Dockerfile -t ${GITUSER}/${GITREPO}:latest .
docker tag ${GITUSER}/${GITREPO}:latest ${GITUSER}/${GITREPO}:v${VERSION}

# Show the list of available images for this repo
echo "Image is ready"
docker images | grep ${GITREPO}

popd
