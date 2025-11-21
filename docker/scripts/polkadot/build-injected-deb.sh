#!/usr/bin/env bash

# Sample call:
# $0 /path/to/folder_with_binary
# This script replace the former dedicated Dockerfile
# and shows how to use the generic binary_injected.dockerfile

PROJECT_ROOT=`git rev-parse --show-toplevel`

echo "========================================"
echo "DEBUG: build-injected-deb.sh is running"
echo "DEBUG: VERSION argument: $1"
echo "========================================"

export BINARY=polkadot,polkadot-execute-worker,polkadot-prepare-worker
export DOCKERFILE="docker/dockerfiles/polkadot/polkadot_injected_debian.Dockerfile"
export POLKADOT_DEB=true
export VERSION=$1

echo "DEBUG: Exported variables:"
echo "  BINARY=$BINARY"
echo "  DOCKERFILE=$DOCKERFILE"
echo "  POLKADOT_DEB=$POLKADOT_DEB"
echo "  VERSION=$VERSION"
echo "========================================"

$PROJECT_ROOT/docker/scripts/build-injected.sh
