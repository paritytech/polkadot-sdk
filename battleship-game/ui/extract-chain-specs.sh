#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(dirname "$0")"
CHAIN_SPECS_DIR="$SCRIPT_DIR/public/chain-specs"

ZOMBIE_DIR=$(find /tmp -maxdepth 1 -name "zombie-*" -type d 2>/dev/null | head -1)

if [ -z "$ZOMBIE_DIR" ]; then
    echo "Error: No zombienet directory found in /tmp"
    echo "Start zombienet first with: cd ../network && ./run.sh"
    exit 1
fi

echo "Found zombienet at: $ZOMBIE_DIR"

mkdir -p "$CHAIN_SPECS_DIR"

if [ -f "$ZOMBIE_DIR/rococo-local.json" ]; then
    cp "$ZOMBIE_DIR/rococo-local.json" "$CHAIN_SPECS_DIR/relay.json"
    echo "Copied relay chain spec"
else
    echo "Error: Relay chain spec not found"
    exit 1
fi

if [ -f "$ZOMBIE_DIR/.json" ]; then
    cp "$ZOMBIE_DIR/.json" "$CHAIN_SPECS_DIR/parachain.json"
    echo "Copied parachain spec"
else
    echo "Error: Parachain spec not found"
    exit 1
fi

echo "Chain specs extracted to: $CHAIN_SPECS_DIR"
ls -lh "$CHAIN_SPECS_DIR"
