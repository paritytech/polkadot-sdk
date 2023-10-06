#!/bin/sh
#
# Runtime benchmarks for the `pallet-bridge-messages` and `pallet-bridge-grandpa` pallets.
#
# Run this script from root of the repo.

set -eux

# default (test) weights that we'll bundle with our pallets

time cargo run --release -p millau-bridge-node --features=runtime-benchmarks -- benchmark pallet \
	--chain=dev \
	--steps=50 \
	--repeat=20 \
	--pallet=pallet_bridge_messages \
	--extrinsic=* \
	--execution=wasm \
	--wasm-execution=Compiled \
	--heap-pages=4096 \
	--output=./modules/messages/src/weights.rs \
	--template=./.maintain/bridge-weight-template.hbs

time cargo run --release -p millau-bridge-node --features=runtime-benchmarks -- benchmark pallet \
	--chain=dev \
	--steps=50 \
	--repeat=20 \
	--pallet=pallet_bridge_grandpa \
	--extrinsic=* \
	--execution=wasm \
	--wasm-execution=Compiled \
	--heap-pages=4096 \
	--output=./modules/grandpa/src/weights.rs \
	--template=./.maintain/bridge-weight-template.hbs

time cargo run --release -p millau-bridge-node --features=runtime-benchmarks -- benchmark pallet \
	--chain=dev \
	--steps=50 \
	--repeat=20 \
	--pallet=pallet_bridge_parachains \
	--extrinsic=* \
	--execution=wasm \
	--wasm-execution=Compiled \
	--heap-pages=4096 \
	--output=./modules/parachains/src/weights.rs \
	--template=./.maintain/bridge-weight-template.hbs

time cargo run --release -p millau-bridge-node --features=runtime-benchmarks -- benchmark pallet \
	--chain=dev \
	--steps=50 \
	--repeat=20 \
	--pallet=pallet_bridge_relayers \
	--extrinsic=* \
	--execution=wasm \
	--wasm-execution=Compiled \
	--heap-pages=4096 \
	--output=./modules/relayers/src/weights.rs \
	--template=./.maintain/bridge-weight-template.hbs

time cargo run --release -p millau-bridge-node --features=runtime-benchmarks -- benchmark pallet \
	--chain=dev \
	--steps=50 \
	--repeat=20 \
	--pallet=pallet_xcm_bridge_hub_router \
	--extrinsic=* \
	--execution=wasm \
	--wasm-execution=Compiled \
	--heap-pages=4096 \
	--output=./modules/xcm-bridge-hub-router/src/weights.rs \
	--template=./.maintain/bridge-weight-template.hbs

# weights for Millau runtime. We want to provide runtime weight overhead for messages calls,
# so we can't use "default" test weights directly - they'll be rejected by our integration tests.

time cargo run --release -p millau-bridge-node --features=runtime-benchmarks -- benchmark pallet \
	--chain=dev \
	--steps=50 \
	--repeat=20 \
	--pallet=pallet_bridge_messages \
	--extrinsic=* \
	--execution=wasm \
	--wasm-execution=Compiled \
	--heap-pages=4096 \
	--output=./bin/millau/runtime/src/weights/

