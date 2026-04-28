#!/usr/bin/env bash
# Register pair 0 (DOT/USD) and pair 1 (BTC/USD) and set their endpoints on a
# running zombienet, wrapped in sudo. On westend-local the sudo key is Alice
# (//Alice).
#
# Requires: polkadot-js-api CLI
#   npm install -g @polkadot/api-cli
#
# Usage: ./add-dot-pair.sh [ws_url]
#   ws_url defaults to ws://127.0.0.1:9944

set -euo pipefail

WS_URL="${1:-ws://127.0.0.1:9944}"
SUDO_SEED="//Alice"

DOT_PAIR_ID=0
BTC_PAIR_ID=1

# PairConfig matches westend defaults: epsilon=0.001 (FixedU128 inner = 10^15),
# min_nudges=0, nudge_validity=2 slots, inherent neither mandatory nor panicking.
PAIR_CONFIG='{"minNudges":0,"nudgeValidity":30,"inherentMandatory":false,"invalidInherentPanics":false,"epsilon":"1000000000000000"}'

# Initial on-chain price seeded at registration (FixedU128 inner; 10^18 = 1.0).
# Quoted as a JSON string so polkadot-js-api parses it as a BigInt — bare
# numbers above 2^53-1 overflow JS Number.
INITIAL_PRICE='"1000000000000000000"'

# BTC Initial price is is 60000 USD
BTC_INITIAL_PRICE='"60000000000000000000"'

# (parsing_method_id, url). Ids come from substrate/frame/price-oracle/src/decoders.rs:
#   0 = Binance, 1 = CoinLore.
DOT_ENDPOINTS='[[0,"https://data-api.binance.vision/api/v3/ticker/price?symbol=DOTUSDT"],[1,"https://api.coinlore.net/api/ticker/?id=45219"]]'
BTC_ENDPOINTS='[[0,"https://data-api.binance.vision/api/v3/ticker/price?symbol=BTCUSDT"],[1,"https://api.coinlore.net/api/ticker/?id=90"]]'

echo "==> Registering pair $DOT_PAIR_ID (DOT) via sudo at $WS_URL"
polkadot-js-api --ws "$WS_URL" --seed "$SUDO_SEED" --sudo \
  tx.priceOracle.registerPair "$DOT_PAIR_ID" "$PAIR_CONFIG" "$INITIAL_PRICE"

echo "==> Setting active endpoints for pair $DOT_PAIR_ID via sudo"
polkadot-js-api --ws "$WS_URL" --seed "$SUDO_SEED" --sudo \
  tx.priceOracle.setActiveEndpoints "$DOT_PAIR_ID" "$DOT_ENDPOINTS"

echo "==> Registering pair $BTC_PAIR_ID (BTC) via sudo at $WS_URL"
polkadot-js-api --ws "$WS_URL" --seed "$SUDO_SEED" --sudo \
  tx.priceOracle.registerPair "$BTC_PAIR_ID" "$PAIR_CONFIG" "$BTC_INITIAL_PRICE"

echo "==> Setting active endpoints for pair $BTC_PAIR_ID via sudo"
polkadot-js-api --ws "$WS_URL" --seed "$SUDO_SEED" --sudo \
  tx.priceOracle.setActiveEndpoints "$BTC_PAIR_ID" "$BTC_ENDPOINTS"

echo "Done."
