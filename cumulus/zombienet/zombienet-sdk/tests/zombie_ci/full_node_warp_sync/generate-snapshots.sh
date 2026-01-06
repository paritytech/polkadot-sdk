#!/bin/bash
# Automation script for updating the full_node_warp_sync test
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELAYCHAIN_DB="relaychain-db.tgz"
PARACHAIN_DB="parachain-db.tgz"

# Config
PARACHAIN_SPEC="$SCRIPT_DIR/warp-sync-parachain-spec.json"
RELAYCHAIN_SPEC="$SCRIPT_DIR/warp-sync-relaychain-spec.json"
TARGET_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release
SNAPSHOT_DIR="${ZOMBIENET_SDK_BASE_DIR:-/tmp/zombienet-warp-sync}"

usage() {
    cat <<EOF
Usage: $0 <phase>

Phases:
  build                  Build all required binaries
  chainspec-parachain    Generate parachain chain spec
  chainspec-relaychain   Generate relaychain chain spec
  snapshots-run          Run test to generate snapshots (24h)
  snapshots-archive      Archive databases into tarballs
  snapshots-test-local   Test with local snapshots
  all                    Run all phases

Environment:
  SNAPSHOT_DIR  Snapshot directory (default: /tmp/zombienet-warp-sync)
EOF
    exit 1
}

build_binaries() {
    echo "==> Building all required binaries"
    cargo build --release -p cumulus-test-runtime
    cargo build --release -p staging-chain-spec-builder
    cargo build --release -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot
    cargo build --release -p cumulus-test-service --bin test-parachain
    echo "Build complete"
}

chainspec_parachain() {
    echo "==> Generating parachain chain spec"

    local wasm_path="$TARGET_DIR/wbuild/cumulus-test-runtime/cumulus_test_runtime.wasm"
    [[ -f "$wasm_path" ]] || { echo "Error: WASM runtime not found at $wasm_path" >&2; exit 1; }

    "$TARGET_DIR/chain-spec-builder" create -r "$wasm_path" named-preset development
    [[ -f "$PARACHAIN_SPEC" ]] && cp "$PARACHAIN_SPEC" "$PARACHAIN_SPEC.backup"
    mv chain_spec.json "$PARACHAIN_SPEC"
    echo "Created: $PARACHAIN_SPEC"
}

chainspec_relaychain() {
    echo "==> Generating relaychain chain spec"

    "$TARGET_DIR/polkadot" build-spec --chain rococo-local --disable-default-bootnode > chain_spec.json
    [[ -f "$RELAYCHAIN_SPEC" ]] && cp "$RELAYCHAIN_SPEC" "$RELAYCHAIN_SPEC.backup"
    mv chain_spec.json "$RELAYCHAIN_SPEC"
    echo "Created: $RELAYCHAIN_SPEC"
}

snapshots_generate() {
    echo "==> Running test to generate snapshots (24h)"
    echo "Output directory: $SNAPSHOT_DIR"

    mkdir -p "$SNAPSHOT_DIR"

    export PATH="$TARGET_DIR:$PATH"
    export RUST_LOG=info,zombienet_orchestrator=debug
    export ZOMBIE_PROVIDER=native
    export ZOMBIENET_SDK_BASE_DIR=$SNAPSHOT_DIR

    echo cargo nextest run --release \
        -p cumulus-zombienet-sdk-tests \
        --features zombie-ci,generate-snapshots \
        --no-capture \
        -- full_node_warp_sync::generate_snapshots
    unset ZOMBIENET_SDK_BASE_DIRG

    echo "Snapshots ready in: $SNAPSHOT_DIR"
}

snapshots_archive() {
    echo "==> Archiving databases"

    [[ -d "$SNAPSHOT_DIR/alice/data" ]] || { echo "Error: alice database not found" >&2; exit 1; }
    [[ -d "$SNAPSHOT_DIR/one/data" ]] || { echo "Error: one database not found" >&2; exit 1; }

    cd "$SCRIPT_DIR"
    tar -czf $RELAYCHAIN_DB -C "$SNAPSHOT_DIR/alice" data/
    tar -czf $PARACHAIN_DB -C "$SNAPSHOT_DIR/one" data/ relay-data/

    echo "Created: ${SCRIPT_DIR}/${RELAYCHAIN_DB} ($(du -h $RELAYCHAIN_DB | cut -f1))"
    echo "Created: ${SCRIPT_DIR}/${PARACHAIN_DB} ($(du -h $PARACHAIN_DB | cut -f1))"
    echo
    echo "Next: $0 snapshots-test-local"
}

snapshots_test_local() {
    echo "==> Testing with local snapshots:"
    echo "- ${SCRIPT_DIR}/${RELAYCHAIN_DB}"
    echo "- ${SCRIPT_DIR}/${PARACHAIN_DB}"

    [[ -f "${SCRIPT_DIR}/${RELAYCHAIN_DB}" ]] || { echo "Error: $RELAYCHAIN_DB not found" >&2; exit 1; }
    [[ -f "${SCRIPT_DIR}/${PARACHAIN_DB}" ]] || { echo "Error: $PARACHAIN_DB not found" >&2; exit 1; }

    export DB_SNAPSHOT_RELAYCHAIN_OVERRIDE="${SCRIPT_DIR}/${RELAYCHAIN_DB}"
    export DB_SNAPSHOT_PARACHAIN_OVERRIDE="${SCRIPT_DIR}/${PARACHAIN_DB}"
    export PATH="$TARGET_DIR:$PATH"
    export RUST_LOG=info,zombienet_orchestrator=debug
    export ZOMBIE_PROVIDER=native

    cargo nextest run --release \
        -p cumulus-zombienet-sdk-tests \
        --features zombie-ci \
        --no-capture \
        -- full_node_warp_sync

    echo "Test passed - snapshots validated"
    echo "Snapshots ready to upload to google storage:"
    echo "- ${SCRIPT_DIR}/${RELAYCHAIN_DB}"
    echo "- ${SCRIPT_DIR}/${PARACHAIN_DB}"
}

all() {
    build_binaries
    chainspec_parachain
    chainspec_relaychain
    snapshots_generate
    snapshots_archive
    snapshots_test_local
    echo "All phases complete"
}

# Main
[[ $# -eq 0 ]] && usage

case "$1" in
    build)                build_binaries ;;
    chainspec-parachain)  chainspec_parachain ;;
    chainspec-relaychain) chainspec_relaychain ;;
    snapshots-generate)   snapshots_generate ;;
    snapshots-archive)    snapshots_archive ;;
    snapshots-test-local) snapshots_test_local ;;
    all)                  all ;;
    *)                    echo "Unknown phase: $1" >&2; usage ;;
esac
