#!/bin/bash

# Script to collect Ethereum block and receipt test data
# Usage: ./get_test_data.sh

# ============================================================================
# CONFIGURATION: Add your blocks to fetch here
# ============================================================================
# Format: "network_identifier|rpc_url|block_number"
# The network_identifier will be used in the filename: block_{block_number}_{network_identifier}.json

# Below blocks were picked because they include all supported transaction types.
BLOCKS_TO_FETCH=(
    # Ethereum mainnet blocks
    "ethereum-mainnet|https://ethereum-rpc.publicnode.com|0x161bd0f"
    "ethereum-mainnet|https://ethereum-rpc.publicnode.com|0x151241d"

    # Sepolia testnet blocks
    "ethereum-sepolia|https://ethereum-sepolia.gateway.tatum.io|0x874db3"
)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Function to fetch block data
fetch_block() {
    local network_id="$1"
    local rpc_url="$2"
    local block_number="$3"

    local block_file="${SCRIPT_DIR}/block_${block_number}_${network_id}.json"
    local receipts_file="${SCRIPT_DIR}/receipts_${block_number}_${network_id}.json"

    # Fetch block
    curl -s -X POST -H "Content-Type: application/json" \
        --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockByNumber\",\"params\":[\"${block_number}\", true],\"id\":1}" \
        "${rpc_url}" | jq '.result' > "$block_file"

    # Fetch receipts
    curl -s -X POST -H "Content-Type: application/json" \
        --data "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockReceipts\",\"params\":[\"${block_number}\"],\"id\":1}" \
        "${rpc_url}" | jq '.result' > "$receipts_file"
}

# ============================================================================
# Main execution
# ============================================================================

for entry in "${BLOCKS_TO_FETCH[@]}"; do
    IFS='|' read -r network_id rpc_url block_number <<< "$entry"
    fetch_block "$network_id" "$rpc_url" "$block_number"
done
