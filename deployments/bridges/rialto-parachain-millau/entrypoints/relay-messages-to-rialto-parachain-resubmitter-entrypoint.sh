#!/bin/bash
set -xeu

sleep 15

# //RialtoParachain.MessagesSender is signing Millau -> RialtoParachain message-send transactions, which are causing problems.
#
# When large message is being sent from Millau to RialtoParachain AND other transactions are
# blocking it from being mined, we'll see something like this in logs:
#
# Millau transaction priority with tip=0: 17800827994. Target priority:
# 526186677695
#
# So since fee multiplier in Millau is `1` and `WeightToFee` is `IdentityFee`, then
# we need tip around `526186677695 - 17800827994 = 508_385_849_701`. Let's round it
# up to `1_000_000_000_000`.

exec /home/user/substrate-relay resubmit-transactions millau \
	--target-host millau-node-alice \
	--target-port 9944 \
	--target-signer //RialtoParachain.MessagesSender \
	--stalled-blocks 7 \
	--tip-limit 1000000000000 \
	--tip-step 1000000000 \
	make-it-best-transaction
