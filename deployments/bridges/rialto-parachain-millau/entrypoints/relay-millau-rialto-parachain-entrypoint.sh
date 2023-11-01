#!/bin/bash
set -xeu

sleep 15

MILLAU_RELAY_ACCOUNT=${EXT_MILLAU_RELAY_ACCOUNT:-//RialtoParachain.HeadersAndMessagesRelay1}
MILLAU_RELAY_ACCOUNT_HEADERS_OVERRIDE=${EXT_MILLAU_RELAY_ACCOUNT_HEADERS_OVERRIDE:-//RialtoParachain.RialtoHeadersRelay1}
RIALTO_PARACHAIN_RELAY_ACCOUNT=${EXT_RIALTO_PARACHAIN_RELAY_ACCOUNT:-//Millau.HeadersAndMessagesRelay1}

/home/user/substrate-relay init-bridge millau-to-rialto-parachain \
	--source-host millau-node-alice \
	--source-port 9944 \
	--target-host rialto-parachain-collator-alice \
	--target-port 9944 \
	--target-signer //Sudo

/home/user/substrate-relay init-bridge rialto-to-millau \
	--source-host rialto-node-alice \
	--source-port 9944 \
	--target-host millau-node-alice \
	--target-port 9944 \
	--target-signer //Sudo

# Give chain a little bit of time to process initialization transaction
sleep 6

exec /home/user/substrate-relay relay-headers-and-messages millau-rialto-parachain \
	--millau-host millau-node-alice \
	--millau-port 9944 \
	--millau-signer $MILLAU_RELAY_ACCOUNT \
	--millau-transactions-mortality=64 \
	--rialto-parachain-host rialto-parachain-collator-charlie \
	--rialto-parachain-port 9944 \
	--rialto-parachain-signer $RIALTO_PARACHAIN_RELAY_ACCOUNT \
	--rialto-parachain-transactions-mortality=64 \
	--rialto-host rialto-node-alice \
	--rialto-port 9944 \
	--lane=00000000 \
	--prometheus-host=0.0.0.0
