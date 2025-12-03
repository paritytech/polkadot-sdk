#!/usr/bin/env bash
set -euo pipefail

RELEASE_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release

export PATH="$RELEASE_DIR:$PATH"
ZOMBIE_PROVIDER=${ZOMBIE_PROVIDER:-native} cargo test --release -p substrate-zombienet-sdk-tests --features zombie-ci "$@"
