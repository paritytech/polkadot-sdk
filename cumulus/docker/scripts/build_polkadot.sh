#!/usr/bin/env bash

set -e

cumulus_repo=$(cd "$(dirname "$0")" && git rev-parse --show-toplevel)
polkadot_repo=$(dirname "$cumulus_repo")/polkadot
if [ ! -d "$polkadot_repo/.git" ]; then
    echo "please clone polkadot in parallel to this repo:"
    echo "  (cd .. && git clone git@github.com:paritytech/polkadot.git)"
    exit 1
fi

if [ -z "$BRANCH" ]; then
    BRANCH=cumulus-branch
fi

cd "$polkadot_repo"
git fetch
git checkout "$BRANCH"
time docker build \
    -f ./docker/Dockerfile \
    --build-arg PROFILE=release \
    -t polkadot:"$BRANCH" .
