#!/bin/bash

echo "🔨 Building Polkadot with runtime-benchmarks feature for XCM generic benchmarks..."
echo

# Build polkadot binary with runtime-benchmarks feature
echo "📦 Building polkadot binary with runtime-benchmarks..."
cargo build --release --features runtime-benchmarks -p polkadot
if [ $? -ne 0 ]; then
    echo "❌ Failed to build polkadot binary with runtime-benchmarks"
    exit 1
fi
echo "✅ polkadot binary built successfully with runtime-benchmarks"
echo

# Run XCM generic benchmarks for Rococo
echo "🏃 Running XCM generic benchmarks for Rococo runtime..."
echo "   This will benchmark all XCM generic instructions including publish and subscribe"
echo

./target/release/polkadot benchmark pallet \
    --chain rococo-local \
    --pallets pallet_xcm_benchmarks::generic \
    --extrinsic '*' \
    --steps 50 \
    --repeat 20 \
    --output ./polkadot/runtime/rococo/src/weights/xcm/pallet_xcm_benchmarks_generic.rs \
    --header ./polkadot/file_header.txt

if [ $? -ne 0 ]; then
    echo "❌ Benchmark execution failed"
    exit 1
fi

echo
echo "✅ Benchmarks completed successfully!"
echo
echo "📍 Generated weight file:"
echo "  - Location: polkadot/runtime/rococo/src/weights/xcm/pallet_xcm_benchmarks_generic.rs"
echo
echo "📊 The weight file now includes:"
echo "  - publish(n) - Linear weight scaling with number of items"
echo "  - subscribe() - Constant weight"
echo "  - All other XCM generic instructions"
echo
echo "🎉 Ready to commit the updated weights!"
