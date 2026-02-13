#!/usr/bin/env bash
# Fast test iteration script - skips rebuilds when possible
# Use this after initial build for maximum speed

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "🧪 Fast Zombienet Test Runner"
echo "============================="
echo ""
echo "This script runs zombienet tests with optimizations for fast iteration:"
echo "  - SKIP_WASM_BUILD=1 (skips WASM rebuilds)"
echo "  - Assumes binaries already built (use ./zombienet-quick-build.sh first)"
echo ""

# Check if binaries exist
TESTNET_DIR="$PROJECT_ROOT/target/testnet"
RELEASE_DIR="$PROJECT_ROOT/target/release"

if [ ! -f "$TESTNET_DIR/polkadot" ] && [ ! -f "$RELEASE_DIR/polkadot" ]; then
    echo "❌ Error: polkadot binary not found!"
    echo "Run ./scripts/zombienet-quick-build.sh first to build binaries"
    exit 1
fi

# Add binaries to PATH
export PATH="$TESTNET_DIR:$RELEASE_DIR:$PATH"

# Enable SKIP_WASM_BUILD unless explicitly disabled
export SKIP_WASM_BUILD=${SKIP_WASM_BUILD:-1}

echo "Environment:"
echo "  SKIP_WASM_BUILD=$SKIP_WASM_BUILD"
echo "  ZOMBIE_PROVIDER=${ZOMBIE_PROVIDER:-native}"
echo "  PATH includes: target/testnet and target/release"
echo ""

# Allow specifying which test suite to run
TEST_SUITE=${1:-cumulus}

case "$TEST_SUITE" in
    cumulus)
        echo "Running cumulus zombienet tests..."
        ZOMBIE_PROVIDER=${ZOMBIE_PROVIDER:-native} cargo test --release \
          -p cumulus-zombienet-sdk-tests --features zombie-ci \
          -- --test-threads 1 "${@:2}"
        ;;
    substrate)
        echo "Running substrate zombienet tests..."
        ZOMBIE_PROVIDER=${ZOMBIE_PROVIDER:-native} cargo test --release \
          -p substrate-zombienet-sdk-tests --features zombie-ci \
          "${@:2}"
        ;;
    polkadot)
        echo "Running polkadot zombienet tests..."
        cargo nextest run --release \
          -p polkadot-zombienet-sdk-tests --features zombie-ci \
          "${@:2}"
        ;;
    *)
        echo "Usage: $0 [cumulus|substrate|polkadot] [additional test args]"
        echo ""
        echo "Examples:"
        echo "  $0 cumulus                    # Run all cumulus tests"
        echo "  $0 cumulus test_name          # Run specific cumulus test"
        echo "  $0 substrate                  # Run substrate tests"
        echo "  $0 polkadot                   # Run polkadot tests"
        exit 1
        ;;
esac

echo ""
echo "✅ Tests complete!"
