#!/bin/bash
set -xeu

sleep 15

MESSAGE_LANE=${MSG_EXCHANGE_GEN_LANE:-00000000}

/home/user/substrate-relay relay-messages millau-to-rialto \
	--lane $MESSAGE_LANE \
	--source-host millau-node-bob \
	--source-port 9944 \
	--source-signer //Rialto.OutboundMessagesRelay.Lane00000001 \
	--target-host rialto-node-bob \
	--target-port 9944 \
	--target-signer //Millau.InboundMessagesRelay.Lane00000001 \
	--prometheus-host=0.0.0.0
