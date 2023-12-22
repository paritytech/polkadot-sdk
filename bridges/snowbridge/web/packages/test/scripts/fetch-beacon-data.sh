#!/usr/bin/env bash
set -eu
source scripts/set-env.sh

# finality_update
curl -s "$beacon_endpoint_http/eth/v1/beacon/light_client/finality_update" | jq -r "." > testdata/finality_update.json
finalized_block_number=$(curl -s "$beacon_endpoint_http/eth/v1/beacon/light_client/finality_update" | jq -r ".data.finalized_header.beacon.slot")
echo "finalized_block_number is: $finalized_block_number"

# get block_root by block_number
finalized_block_root=$(curl -s "$beacon_endpoint_http/eth/v1/beacon/blocks/$finalized_block_number/root" | jq -r ".data.root")
echo "finalized_block_root is: $finalized_block_root"

# get beacon header by block_root
beacon_header=$(curl -s "$beacon_endpoint_http/eth/v1/beacon/headers/$finalized_block_root")
echo $beacon_header

# get beacon block by block_root
curl -s "$beacon_endpoint_http/eth/v2/beacon/blocks/$finalized_block_root" | jq -r "." > testdata/beacon_block.json

# get beacon state by block_number
curl -s "$beacon_endpoint_http/eth/v2/debug/beacon/states/$finalized_block_number" | jq -r "." > testdata/beacon_state.json
