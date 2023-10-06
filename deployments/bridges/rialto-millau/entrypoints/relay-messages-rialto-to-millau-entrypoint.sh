#!/bin/bash
set -xeu

sleep 15

MESSAGE_LANE=${MSG_EXCHANGE_GEN_LANE:-00000000}

/home/user/substrate-relay relay-messages rialto-to-millau \
	--lane $MESSAGE_LANE \
	--source-host rialto-node-bob \
	--source-port 9944 \
	--source-signer //Millau.OutboundMessagesRelay.Lane00000001 \
	--target-host millau-node-bob \
	--target-port 9944 \
	--target-signer //Rialto.InboundMessagesRelay.Lane00000001 \
	--prometheus-host=0.0.0.0
