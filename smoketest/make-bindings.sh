#!/usr/bin/env bash

set -eu

mkdir -p src/contracts

# Generate Rust bindings for contracts
forge bind --module --overwrite \
    --select 'IGateway|WETH9|GatewayUpgradeMock' \
    --bindings-path src/contracts \
    --root ../contracts

# Install subxt
command -v subxt || cargo install subxt-cli \
    --git https://github.com/paritytech/subxt.git \
    --tag v0.27.1

if ! lsof -Pi :11144 -sTCP:LISTEN -t >/dev/null; then
    echo "substrate nodes not running, please start with the e2e setup and rerun this script"
    exit 1
fi

# Fetch metadata from BridgeHub and generate client
subxt codegen --url ws://localhost:11144 > src/parachains/bridgehub.rs
subxt codegen --url ws://localhost:12144 > src/parachains/assethub.rs
subxt codegen --url ws://localhost:13144 > src/parachains/penpal.rs
subxt codegen --url ws://localhost:9944  > src/parachains/relaychain.rs
