#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(dirname "$0")"

cargo build --release -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot -p polkadot-omni-node --bin polkadot-omni-node -p battleship-runtime

RELEASE_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release

export PATH=$RELEASE_DIR:$PATH

zombie-cli spawn --provider native battleship.toml
