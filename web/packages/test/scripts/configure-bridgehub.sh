#!/usr/bin/env bash
set -eu

source scripts/set-env.sh
source scripts/xcm-helper.sh

config_beacon_checkpoint() {
    pushd $root_dir
    local check_point_call=$($relay_bin generate-beacon-checkpoint --spec $active_spec --url $beacon_endpoint_http)
    popd
    send_governance_transact_from_relaychain $BRIDGE_HUB_PARAID "$check_point_call" 180000000000 900000
}

wait_beacon_chain_ready() {
    local initial_beacon_block=""
    while [ -z "$initial_beacon_block" ] || [ "$initial_beacon_block" == "0x0000000000000000000000000000000000000000000000000000000000000000" ]; do
        echo "Waiting for beacon chain to finalize to get initial block..."
        initial_beacon_block=$(curl -s "$beacon_endpoint_http/eth/v1/beacon/states/head/finality_checkpoints" |
            jq -r '.data.finalized.root' || true)
        sleep 3
    done
}

fund_accounts() {
    echo "Funding substrate accounts"
    transfer_balance $relaychain_ws_url "//Charlie" 1013 1000000000000000 $assethub_sovereign_account
    transfer_balance $relaychain_ws_url "//Charlie" 1013 1000000000000000 $penpal_sovereign_account
    transfer_balance $relaychain_ws_url "//Charlie" 1013 1000000000000000 $beacon_relayer_pub_key
    transfer_balance $relaychain_ws_url "//Charlie" 1013 1000000000000000 $execution_relayer_pub_key
}

open_hrmp_channel()
{
    local relay_url=$1
    local relay_chain_seed=$2
    local sender_para_id=$3
    local recipient_para_id=$4
    local max_capacity=$5
    local max_message_size=$6
    echo "  calling open_hrmp_channels:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      sender_para_id: ${sender_para_id}"
    echo "      recipient_para_id: ${recipient_para_id}"
    echo "      max_capacity: ${max_capacity}"
    echo "      max_message_size: ${max_message_size}"
    echo "      params:"
    echo "--------------------------------------------------"

    npx polkadot-js-api \
        --ws "${relay_url?}" \
        --seed "${relay_chain_seed?}" \
        --sudo \
        tx.hrmp.forceOpenHrmpChannel \
            ${sender_para_id} \
            ${recipient_para_id} \
            ${max_capacity} \
            ${max_message_size}
}

open_hrmp_channels()
{
    echo "Opening HRMP channels"
    open_hrmp_channel "${relaychain_ws_url}" "${relaychain_sudo_seed}" 1000 1013 8 512 # Assethub -> BridgeHub
    open_hrmp_channel "${relaychain_ws_url}"  "${relaychain_sudo_seed}" 1013 1000 8 512 # BridgeHub -> Assethub
    open_hrmp_channel "${relaychain_ws_url}" "${relaychain_sudo_seed}" 2000 1013 8 512 # Penpal -> BridgeHub
    open_hrmp_channel "${relaychain_ws_url}" "${relaychain_sudo_seed}" 1013 2000 8 512 # BridgeHub -> Penpal
    open_hrmp_channel "${relaychain_ws_url}" "${relaychain_sudo_seed}" 1000 2000 8 512 # Penpal -> AssetHub
    open_hrmp_channel "${relaychain_ws_url}" "${relaychain_sudo_seed}" 2000 1000 8 512 # Assethub -> Penpal
}

configure_bridgehub() {
    fund_accounts
    wait_beacon_chain_ready
    config_beacon_checkpoint
    open_hrmp_channels
}

if [ -z "${from_start_services:-}" ]; then
    echo "config beacon checkpoint only!"
    configure_bridgehub
    wait
fi
