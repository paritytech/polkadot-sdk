#!/usr/bin/env bash
# Quick build script for zombienet tests - only builds what's actually needed
# This significantly reduces build time compared to building all runtimes

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "🚀 Quick Build for Zombienet Tests"
echo "===================================="
echo ""
echo "This script builds only the binaries needed for most zombienet tests:"
echo "  - polkadot with ONLY rococo runtime (skips westend for ~5 min savings)"
echo "  - polkadot-parachain (still builds all runtimes - see Phase 2 for omninode migration)"
echo "  - test-parachain (needed for cumulus tests)"
echo ""

# Build polkadot with only rococo runtime
echo "📦 Building polkadot + workers (rococo-only, testnet profile)..."
cargo build --profile testnet --no-default-features --features rococo-native,fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker

echo ""
echo "📦 Building polkadot-parachain (release profile)..."
cargo build --release --locked -p polkadot-parachain-bin --bin polkadot-parachain

echo ""
echo "📦 Building test-parachain (release profile)..."
cargo build --release --locked -p cumulus-test-service --bin test-parachain

echo ""
echo "✅ Build complete!"
echo ""
echo "Binaries located in:"
echo "  - target/testnet/polkadot*"
echo "  - target/release/polkadot-parachain"
echo "  - target/release/test-parachain"
echo ""
echo "💡 Tips for faster iteration:"
echo "  - Use SKIP_WASM_BUILD=1 on subsequent runs if you haven't changed runtimes"
echo "  - For even faster builds, see Phase 2 omninode migration in the plan"
echo ""
echo "To run tests:"
echo "  export PATH=\"$PROJECT_ROOT/target/testnet:$PROJECT_ROOT/target/release:\$PATH\""
echo "  ZOMBIE_PROVIDER=native cargo test -p cumulus-zombienet-sdk-tests --features zombie-ci"
