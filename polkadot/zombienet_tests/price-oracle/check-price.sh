#!/usr/bin/env bash
# Query the on-chain price from the price oracle pallet for a given pair.
#
# Requires: polkadot-js-api CLI
#   npm install -g @polkadot/api-cli
#
# Usage: ./check-price.sh [ws_url] [pair_id]
#   ws_url  defaults to ws://127.0.0.1:9944
#   pair_id defaults to 0 (DOT/USD)
#
# CurrentPrice is a StorageMap<PairId, FixedU128>, so the value is returned as
# a FixedU128 inner u128 (10^18 = 1.0).

set -euo pipefail

WS_URL="${1:-ws://127.0.0.1:9944}"
PAIR_ID="${2:-0}"

echo "Querying priceOracle.currentPrice($PAIR_ID) at $WS_URL..."
polkadot-js-api --ws "$WS_URL" query.priceOracle.currentPrice "$PAIR_ID"
