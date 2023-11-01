#!/bin/bash
set -eu

sleep 15

exec /home/user/substrate-relay detect-equivocations millau-to-rialto \
	--source-host millau-node-alice \
	--source-port 9944 \
	--source-signer //Rialto.HeadersAndMessagesRelay \
	--source-transactions-mortality=64 \
	--target-host rialto-node-alice \
	--target-port 9944 \
	--prometheus-host=0.0.0.0
