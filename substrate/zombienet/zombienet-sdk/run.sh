#!/usr/bin/env bash
set -euo pipefail

# Quick iteration tip: Set SKIP_WASM_BUILD=1 to skip WASM rebuilds if you haven't changed runtime code
# Example: SKIP_WASM_BUILD=1 ./run.sh

RELEASE_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release

export PATH="$RELEASE_DIR:$PATH"
ZOMBIE_PROVIDER=${ZOMBIE_PROVIDER:-native} cargo test --release -p substrate-zombienet-sdk-tests --features zombie-ci "$@"
