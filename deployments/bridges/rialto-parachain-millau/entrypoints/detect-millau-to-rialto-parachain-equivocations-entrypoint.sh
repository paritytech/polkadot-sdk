#!/bin/bash
set -eu

sleep 15

exec /home/user/substrate-relay detect-equivocations millau-to-rialto-parachain \
	--source-host millau-node-alice \
	--source-port 9944 \
	--source-signer //RialtoParachain.HeadersAndMessagesRelay1 \
	--source-transactions-mortality=64 \
	--target-host rialto-parachain-collator-charlie \
	--target-port 9944 \
	--prometheus-host=0.0.0.0
