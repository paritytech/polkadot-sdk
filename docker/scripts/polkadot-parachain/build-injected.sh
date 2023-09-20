#!/usr/bin/env bash

# Sample call:
# $0 /path/to/folder_with_binary
# This script replace the former dedicated Dockerfile
# and shows how to use the generic binary_injected.dockerfile

PROJECT_ROOT=`git rev-parse --show-toplevel`

export BINARY=polkadot-parachain
export ARTIFACTS_FOLDER=$1
export DOCKERFILE="docker/dockerfiles/polkadot-parachain/polkadot-parachain_injected.Dockerfile"
# export TAGS=...

$PROJECT_ROOT/docker/scripts/build-injected.sh
