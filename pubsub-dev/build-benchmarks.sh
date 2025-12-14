#!/bin/bash

export DYLD_LIBRARY_PATH=/Library/Developer/CommandLineTools/usr/lib
export SKIP_PALLET_REVIVE_FIXTURES=1

echo "🔨 Building Polkadot SDK binaries with runtime-benchmarks feature..."
echo

# Build main polkadot binary with runtime-benchmarks
echo "📦 Building polkadot relay chain binary (with runtime-benchmarks)..."
cargo build --release -p polkadot --bin polkadot --features runtime-benchmarks
if [ $? -ne 0 ]; then
    echo "❌ Failed to build polkadot binary"
    exit 1
fi
echo "✅ polkadot binary built successfully"
echo

# Build PVF execute worker
echo "📦 Building polkadot-execute-worker..."
cargo build --release -p polkadot --bin polkadot-execute-worker
if [ $? -ne 0 ]; then
    echo "❌ Failed to build polkadot-execute-worker"
    exit 1
fi
echo "✅ polkadot-execute-worker built successfully"
echo

# Build PVF prepare worker
echo "📦 Building polkadot-prepare-worker..."
cargo build --release -p polkadot --bin polkadot-prepare-worker
if [ $? -ne 0 ]; then
    echo "❌ Failed to build polkadot-prepare-worker"
    exit 1
fi
echo "✅ polkadot-prepare-worker built successfully"
echo

# Build parachain binary with runtime-benchmarks
echo "📦 Building polkadot-parachain binary (with runtime-benchmarks)..."
cargo build --release -p polkadot-parachain-bin --bin polkadot-parachain --features runtime-benchmarks
if [ $? -ne 0 ]; then
    echo "❌ Failed to build polkadot-parachain binary"
    exit 1
fi
echo "✅ polkadot-parachain binary built successfully"
echo

echo "🎉 All binaries built successfully with runtime-benchmarks!"
echo
echo "📍 Binary locations:"
echo "  - Relay chain: target/release/polkadot"
echo "  - Execute worker: target/release/polkadot-execute-worker"
echo "  - Prepare worker: target/release/polkadot-prepare-worker"
echo "  - Parachain: target/release/polkadot-parachain"
echo
echo "🚀 Ready for benchmarking!"
