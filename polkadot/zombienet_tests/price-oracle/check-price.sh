#!/usr/bin/env bash
# Query the on-chain price from the price oracle pallet via RPC.
# Usage: ./check-price.sh [rpc_url]
#
# Requires: curl, jq
#
# The price is stored at PriceOracle::CurrentPrice in the westend runtime.
# Pallet index 107, storage CurrentPrice (twox128("PriceOracle") ++ twox128("CurrentPrice"))

RPC_URL="${1:-http://localhost:9944}"

# Storage key for PriceOracle::CurrentPrice
# twox128("PriceOracle") = 0x767e1383ca626a0834244523d01cb352
# twox128("CurrentPrice") = 0x7b1dfbeaa7e3c23c1a68e0a15b2c45db
STORAGE_KEY="0x767e1383ca626a0834244523d01cb3527b1dfbeaa7e3c23c1a68e0a15b2c45db"

echo "Querying price oracle at $RPC_URL..."

RESULT=$(curl -s -H "Content-Type: application/json" -d "{
  \"id\": 1,
  \"jsonrpc\": \"2.0\",
  \"method\": \"state_getStorage\",
  \"params\": [\"$STORAGE_KEY\"]
}" "$RPC_URL")

HEX_VALUE=$(echo "$RESULT" | jq -r '.result // "null"')

if [ "$HEX_VALUE" = "null" ] || [ -z "$HEX_VALUE" ]; then
  echo "Price: 0 (not yet set)"
else
  echo "Raw storage value: $HEX_VALUE"
  echo "(Decode as FixedU128 - little-endian u128 with 18 decimal places)"
fi
