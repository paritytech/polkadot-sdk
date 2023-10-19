#!/bin/bash

# Run a development instance of the Millau Substrate bridge node.
# To override the default port just export MILLAU_PORT=9945

MILLAU_PORT="${MILLAU_PORT:-9945}"

RUST_LOG=runtime=trace \
./target/debug/millau-bridge-node --dev --tmp \
    --rpc-cors=all --unsafe-rpc-external --unsafe-rpc-external \
    --port 33044 --rpc-port $MILLAU_PORT \
