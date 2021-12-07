#!/usr/bin/env bash

OWNER=${OWNER:-parity}
IMAGE_NAME=${IMAGE_NAME:-polkadot-collator}
docker build --no-cache --build-arg IMAGE_NAME=$IMAGE_NAME -t $OWNER/$IMAGE_NAME -f ./docker/injected.Dockerfile .
docker images | grep $IMAGE_NAME
