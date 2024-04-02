#!/bin/sh
set -eux

if [ -z "${LOCAL:-}" ]; then
    time docker build . -t local/substrate-relay --build-arg=PROJECT=substrate-relay
else
    if [ -z "${SKIP_BUILD:-}" ]; then
        time cargo build -p substrate-relay --release
    fi

    # (try to) use docker image matching the host os
    export UBUNTU_RELEASE=`lsb_release -r -s`

    # following (using DOCKER_BUILDKIT) requires docker 19.03 or above
    DOCKER_BUILDKIT=1 time docker build . -f local.Dockerfile -t local/substrate-relay \
        --build-arg=PROJECT=substrate-relay \
        --build-arg=UBUNTU_RELEASE=${UBUNTU_RELEASE}
fi
