#!/bin/bash
set -xeu

sleep 15

RELAY_ACCOUNT=${EXT_RELAY_ACCOUNT:-//Westend.AssetHubWestendHeaders1}

/home/user/substrate-relay relay-parachains westend-to-millau \
	--source-host westend-rpc.polkadot.io \
	--source-port 443 \
	--source-secure \
	--target-host millau-node-alice \
	--target-port 9944 \
	--target-signer $RELAY_ACCOUNT \
	--target-transactions-mortality=4\
	--prometheus-host=0.0.0.0
