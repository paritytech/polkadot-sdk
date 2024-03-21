#!/usr/bin/env bash

OWNER=${OWNER:-parity}
IMAGE_NAME=${IMAGE_NAME:-polkadot-parachain}

docker build --no-cache \
    --build-arg IMAGE_NAME=$IMAGE_NAME \
    -t $OWNER/$IMAGE_NAME \
    -f ./docker/dockerfiles/polkadot-parachain/polkadot-parachain_injected.Dockerfile \
    . && docker images
