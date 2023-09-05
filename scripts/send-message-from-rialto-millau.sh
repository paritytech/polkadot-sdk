#!/bin/bash

# Used for manually sending a message to a running network.
#
# You could for example spin up a full network using the Docker Compose files
# we have (to make sure the message relays are running), but remove the message
# generator service. From there you may submit messages manually using this script.

# TODO: Fix demeo scripts https://github.com/paritytech/parity-bridges-common/issues/1406

RIALTO_PORT="${RIALTO_PORT:-9944}"

RUST_LOG=runtime=trace,substrate-relay=trace,bridge=trace \
./target/debug/substrate-relay send-message rialto-to-millau \
	--source-host localhost \
	--source-port $RIALTO_PORT \
	--source-signer //Bob \
	raw 030426030109030419a8
