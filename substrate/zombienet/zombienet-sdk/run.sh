#!/usr/bin/env bash
set -e

cargo build --release -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot

RELEASE_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release

export PATH=$RELEASE_DIR:$PATH
ZOMBIE_PROVIDER=native cargo test --release -p substrate-zombienet-sdk-tests --features zombie-ci "$@"
