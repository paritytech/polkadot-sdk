#!/bin/sh
set -eux

if [ -z "${LOCAL:-}" ]; then
    time docker build . -t local/substrate-relay --build-arg=PROJECT=substrate-relay
    time docker build . -t local/rialto-bridge-node --build-arg=PROJECT=rialto-bridge-node
    time docker build . -t local/millau-bridge-node --build-arg=PROJECT=millau-bridge-node
    time docker build . -t local/rialto-parachain-collator --build-arg=PROJECT=rialto-parachain-collator
else
    if [ -z "${SKIP_BUILD:-}" ]; then
        time cargo build -p substrate-relay -p rialto-bridge-node -p millau-bridge-node -p rialto-parachain-collator --release
    fi

    # (try to) use docker image matching the host os
    export UBUNTU_RELEASE=`lsb_release -r -s`

    # following (using DOCKER_BUILDKIT) requires docker 19.03 or above
    DOCKER_BUILDKIT=1 time docker build . -f local.Dockerfile -t local/substrate-relay \
        --build-arg=PROJECT=substrate-relay \
        --build-arg=UBUNTU_RELEASE=${UBUNTU_RELEASE}
    DOCKER_BUILDKIT=1 time docker build . -f local.Dockerfile -t local/rialto-bridge-node \
        --build-arg=PROJECT=rialto-bridge-node \
        --build-arg=UBUNTU_RELEASE=${UBUNTU_RELEASE}
    DOCKER_BUILDKIT=1 time docker build . -f local.Dockerfile -t local/millau-bridge-node \
        --build-arg=PROJECT=millau-bridge-node \
        --build-arg=UBUNTU_RELEASE=${UBUNTU_RELEASE}
    DOCKER_BUILDKIT=1 time docker build . -f local.Dockerfile -t local/rialto-parachain-collator \
        --build-arg=PROJECT=rialto-parachain-collator \
        --build-arg=UBUNTU_RELEASE=${UBUNTU_RELEASE}
fi
