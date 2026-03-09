#!/usr/bin/env bash
# Build the battleship UI into a single index.html file.
# Usage: ./build-game-ui.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$SCRIPT_DIR/ui"
npm run build

echo ""
echo "Built: $SCRIPT_DIR/ui/dist/index.html"
