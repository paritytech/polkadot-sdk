#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(dirname "$0")"
cd "$SCRIPT_DIR"

SMOLDOT_DIR="../../../smoldot/wasm-node/javascript"
if [ ! -d "$SMOLDOT_DIR/dist" ] || [[ "${1:-}" == "--rebuild-smoldot" ]]; then
    echo "Building smoldot..."
    (cd "$SMOLDOT_DIR" && npm install && npm run build)
fi

if [[ "${1:-}" == "--extract-chain-specs" ]] || [ ! -f "public/chain-specs/relay.json" ]; then
    ./extract-chain-specs.sh
fi

if [[ "${1:-}" == "--extract-chain-specs" ]] || [ ! -f "src/chain/chainSpecs.ts" ] || [ "public/chain-specs/relay.json" -nt "src/chain/chainSpecs.ts" ]; then
    ./generate-chain-specs.sh
fi

npm install
npm run build

echo "Build complete: dist/index.html"
