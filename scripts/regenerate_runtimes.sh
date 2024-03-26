#!/bin/bash

cd tools/runtime-codegen
cargo run --bin runtime-codegen -- --from-node-url "wss://rococo-bridge-hub-rpc.polkadot.io:443" > ../../relays/client-bridge-hub-rococo/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://rococo-rpc.polkadot.io:443" > ../../relays/client-rococo/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://westend-rpc.polkadot.io:443" > ../../relays/client-westend/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://kusama-rpc.polkadot.io:443" > ../../relays/client-kusama/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://rpc.polkadot.io:443" > ../../relays/client-polkadot/src/codegen_runtime.rs

# Uncomment to update other runtimes

# For `polkadot-sdk` testnet runtimes:
# TODO: there is a bug, probably needs to update subxt, generates: `::sp_runtime::generic::Header<::core::primitive::u32>` withtout second `Hash` parameter.
# cargo run --bin runtime-codegen -- --from-wasm-file ../../../polkadot-sdk/target/release/wbuild/bridge-hub-rococo-runtime/bridge_hub_rococo_runtime.compact.compressed.wasm > ../../relays/client-bridge-hub-rococo/src/codegen_runtime.rs
# cargo run --bin runtime-codegen -- --from-wasm-file ../../../polkadot-sdk/target/release/wbuild/bridge-hub-westend-runtime/bridge_hub_westend_runtime.compact.compressed.wasm > ../../relays/client-bridge-hub-westend/src/codegen_runtime.rs

cd -
cargo fmt --all

# Polkadot Bulletin Chain:
#
# git clone https://github.com/zdave-parity/polkadot-bulletin-chain.git
# cd polkadot-bulletin-chain
# cargo run
# cargo run --bin runtime-codegen -- --from-node-url "ws://127.0.0.1:9944" > ../../relays/client-polkadot-bulletin/src/codegen_runtime.rs