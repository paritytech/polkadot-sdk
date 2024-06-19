#!/usr/bin/env bash

# Example command for updating pallet benchmarking
pushd ../cumulus
cargo run --release --bin polkadot-parachain \
--features runtime-benchmarks \
-- \
benchmark pallet \
--chain=bridge-hub-rococo-dev \
--pallet=snowbridge_pallet_ethereum_client \
--extrinsic="*" \
--execution=wasm --wasm-execution=compiled \
--steps 50 --repeat 20 \
--output ./parachains/runtimes/bridge-hubs/bridge-hub-rococo/src/weights/snowbridge_pallet_ethereum_client.rs
popd
