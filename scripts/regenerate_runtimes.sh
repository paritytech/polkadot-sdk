#!/bin/bash

cd tools/runtime-codegen
cargo run --bin runtime-codegen -- --from-node-url "wss://rococo-bridge-hub-rpc.polkadot.io:443" > ../../relays/client-bridge-hub-rococo/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "http://localhost:20433" > ../../relays/client-rialto-parachain/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://rococo-rpc.polkadot.io:443" > ../../relays/client-rococo/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://kusama-rpc.polkadot.io:443" > ../../relays/client-kusama/src/codegen_runtime.rs
cargo run --bin runtime-codegen -- --from-node-url "wss://rpc.polkadot.io:443" > ../../relays/client-polkadot/src/codegen_runtime.rs
cd -
cargo fmt --all

# Uncomment to update other runtimes

# Polkadot Bulletin Chain:
#
# git clone https://github.com/zdave-parity/polkadot-bulletin-chain.git
# cd polkadot-bulletin-chain
# cargo run
# cargo run --bin runtime-codegen -- --from-node-url "ws://127.0.0.1:9944" > ../../relays/client-polkadot-bulletin/src/codegen_runtime.rs