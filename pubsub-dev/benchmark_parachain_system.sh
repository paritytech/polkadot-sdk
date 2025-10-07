#!/bin/bash

echo "🔨 Building polkadot-parachain with runtime-benchmarks feature for parachain-system benchmarks..."
echo

# Build polkadot-parachain binary with runtime-benchmarks feature
echo "📦 Building polkadot-parachain binary with runtime-benchmarks..."
cargo build --release --features runtime-benchmarks -p polkadot-parachain-bin
if [ $? -ne 0 ]; then
    echo "❌ Failed to build polkadot-parachain binary with runtime-benchmarks"
    exit 1
fi
echo "✅ polkadot-parachain binary built successfully with runtime-benchmarks"
echo

# Run parachain-system benchmarks for westmint (asset-hub-westend)
echo "🏃 Running parachain-system benchmarks for westmint-dev runtime..."
echo "   This will benchmark process_published_data and other parachain-system functions"
echo

./target/release/polkadot-parachain benchmark pallet \
    --chain westmint-dev \
    --pallet cumulus_pallet_parachain_system \
    --extrinsic '*' \
    --execution wasm \
    --wasm-execution compiled \
    --steps 50 \
    --repeat 20 \
    --output ./cumulus/pallets/parachain-system/src/weights.rs \
    --template ./substrate/.maintain/frame-weight-template.hbs

if [ $? -ne 0 ]; then
    echo "❌ Benchmark execution failed"
    exit 1
fi

echo
echo "✅ Benchmarks completed successfully!"
echo
echo "📍 Generated weight file:"
echo "  - Location: cumulus/pallets/parachain-system/src/weights.rs"
echo
echo "📊 The weight file now includes:"
echo "  - enqueue_inbound_downward_messages(n) - DMP message processing"
echo "  - process_published_data(p, k, v) - Published data processing with 3 parameters"
echo
echo "🎉 Ready to commit the updated weights!"
