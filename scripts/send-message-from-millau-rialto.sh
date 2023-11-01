#!/bin/bash

# Used for manually sending a message to a running network.
#
# You could for example spin up a full network using the Docker Compose files
# we have (to make sure the message relays are running), but remove the message
# generator service. From there you may submit messages manually using this script.

# TODO: Fix demeo scripts https://github.com/paritytech/parity-bridges-common/issues/1406

MILLAU_PORT="${MILLAU_PORT:-9945}"

RUST_LOG=runtime=trace,substrate-relay=trace,bridge=trace \
./target/debug/substrate-relay send-message millau-to-rialto \
	--source-host localhost \
	--source-port $MILLAU_PORT \
	--source-signer //Alice \
	raw 030426020109020419a8
