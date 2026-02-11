#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(dirname "$0")"
cd "$SCRIPT_DIR"

CHAIN_SPECS_DIR="public/chain-specs"
OUTPUT_FILE="src/chain/chainSpecs.ts"

if [ ! -f "$CHAIN_SPECS_DIR/relay.json" ] || [ ! -f "$CHAIN_SPECS_DIR/parachain.json" ]; then
    echo "Error: Chain specs not found. Run ./extract-chain-specs.sh first"
    exit 1
fi

echo "Generating $OUTPUT_FILE..."

{
    echo 'export const relayChainSpec = `'
    cat "$CHAIN_SPECS_DIR/relay.json" | sed 's/\\/\\\\/g' | sed 's/`/\\`/g' | sed 's/\$/\\$/g'
    echo '`;'
    echo ''
    echo 'export const parachainSpec = `'
    cat "$CHAIN_SPECS_DIR/parachain.json" | sed 's/\\/\\\\/g' | sed 's/`/\\`/g' | sed 's/\$/\\$/g'
    echo '`;'
} > "$OUTPUT_FILE"

echo "Generated $OUTPUT_FILE ($(wc -c < "$OUTPUT_FILE" | xargs) bytes)"
