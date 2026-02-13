#!/usr/bin/env bash
# Generate chain specs for glutton runtime to use with omninode
# This script creates chain specs with custom glutton configurations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CHAIN_SPEC_BUILDER="${PROJECT_ROOT}/target/release/chain-spec-builder"
OUTPUT_DIR="${PROJECT_ROOT}/zombienet-chain-specs"

cd "$PROJECT_ROOT"

echo "🔧 Glutton Chain Spec Generator for Omninode"
echo "=============================================="
echo ""

# Check for required binaries
if [ ! -f "$CHAIN_SPEC_BUILDER" ]; then
    echo "❌ Error: chain-spec-builder not found at $CHAIN_SPEC_BUILDER"
    echo "Build it with: cargo build --release -p staging-chain-spec-builder"
    exit 1
fi

# Build glutton runtime if needed
RUNTIME_WASM="${PROJECT_ROOT}/target/release/wbuild/glutton-westend-runtime/glutton_westend_runtime.compact.compressed.wasm"
if [ ! -f "$RUNTIME_WASM" ]; then
    echo "📦 Building glutton-westend-runtime..."
    cargo build --release -p glutton-westend-runtime
    echo ""
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "Generating glutton chain specs for para-ids 2000-2001..."
echo ""

# Generate chain specs for para-ids 2000 and 2001
# These match the configuration used in zombienet test 0013-systematic-chunk-recovery.toml
for PARA_ID in 2000 2001; do
    echo "📝 Generating chain spec for glutton-westend-local-${PARA_ID}..."

    # Create a JSON patch file for the glutton configuration
    # This matches the config in the zombienet test:
    #   compute = "50000000"
    #   storage = "2500000000"
    #   trashDataCount = 5120
    PATCH_FILE="${OUTPUT_DIR}/glutton-patch-${PARA_ID}.json"
    cat > "$PATCH_FILE" << EOF
{
  "glutton": {
    "compute": "50000000",
    "storage": "2500000000",
    "trashDataCount": 5120
  }
}
EOF

    # Generate chain spec using the preset and patch
    "$CHAIN_SPEC_BUILDER" create \
        --relay-chain "rococo-local" \
        --para-id "$PARA_ID" \
        -r "$RUNTIME_WASM" \
        patch "$PATCH_FILE" \
        > "${OUTPUT_DIR}/glutton-westend-local-${PARA_ID}-spec.json"

    echo "✅ Created: ${OUTPUT_DIR}/glutton-westend-local-${PARA_ID}-spec.json"

    # Clean up patch file
    rm "$PATCH_FILE"
done

echo ""
echo "✅ Chain spec generation complete!"
echo ""
echo "Generated files:"
ls -lh "${OUTPUT_DIR}/"*.json
echo ""
echo "Usage in zombienet:"
echo "  [[parachains]]"
echo "  id = 2000"
echo "  chain_spec_path = \"${OUTPUT_DIR}/glutton-westend-local-2000-spec.json\""
echo ""
echo "  [parachains.collator]"
echo "  command = \"polkadot-omni-node\""
