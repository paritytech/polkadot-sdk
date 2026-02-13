#!/usr/bin/env bash
set -e

# Quick iteration tips:
# 1. Use ../../../scripts/zombienet-quick-build.sh for optimized builds (rococo-only, saves ~5 min)
# 2. Set SKIP_WASM_BUILD=1 to skip WASM rebuilds if you haven't changed runtime code
# Example: SKIP_WASM_BUILD=1 ./run.sh (assumes binaries already built)

# Build all required binaries
# Use --no-default-features --features rococo-native on polkadot to save ~5 minutes (most tests use rococo-local)
cargo build --release \
  -p cumulus-test-service --bin test-parachain \
  -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot \
  -p polkadot-parachain-bin --bin polkadot-parachain

RELEASE_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release

export PATH=$RELEASE_DIR:$PATH
ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci -- --test-threads 1 "$@"
