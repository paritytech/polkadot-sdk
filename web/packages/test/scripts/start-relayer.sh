#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

config_relayer() {
    # Configure beefy relay
    jq \
        --arg k1 "$(address_for BeefyClient)" \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg eth_gas_limit $eth_gas_limit \
        '
      .sink.contracts.BeefyClient = $k1
    | .source.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum."gas-limit" = $eth_gas_limit
    ' \
        config/beefy-relay.json >$output_dir/beefy-relay.json

    # Configure parachain relay (primary governance)
    jq \
        --arg k1 "$(address_for GatewayProxy)" \
        --arg k2 "$(address_for BeefyClient)" \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg channelID $PRIMARY_GOVERNANCE_CHANNEL_ID \
        --arg eth_gas_limit $eth_gas_limit \
        '
      .source.contracts.Gateway = $k1
    | .source.contracts.BeefyClient = $k2
    | .sink.contracts.Gateway = $k1
    | .source.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum."gas-limit" = $eth_gas_limit
    | .source."channel-id" = $channelID
    ' \
        config/parachain-relay.json >$output_dir/parachain-relay-bridge-hub-01.json

    # Configure parachain relay (secondary governance)
    jq \
        --arg k1 "$(address_for GatewayProxy)" \
        --arg k2 "$(address_for BeefyClient)" \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg channelID $SECONDARY_GOVERNANCE_CHANNEL_ID \
        --arg eth_gas_limit $eth_gas_limit \
        '
      .source.contracts.Gateway = $k1
    | .source.contracts.BeefyClient = $k2
    | .sink.contracts.Gateway = $k1
    | .source.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum."gas-limit" = $eth_gas_limit
    | .source."channel-id" = $channelID
    ' \
        config/parachain-relay.json >$output_dir/parachain-relay-bridge-hub-02.json

    # Configure parachain relay (asset hub)
    jq \
        --arg k1 "$(address_for GatewayProxy)" \
        --arg k2 "$(address_for BeefyClient)" \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg channelID $ASSET_HUB_CHANNEL_ID \
        --arg eth_gas_limit $eth_gas_limit \
        '
      .source.contracts.Gateway = $k1
    | .source.contracts.BeefyClient = $k2
    | .sink.contracts.Gateway = $k1
    | .source.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum."gas-limit" = $eth_gas_limit
    | .source."channel-id" = $channelID
    ' \
        config/parachain-relay.json >$output_dir/parachain-relay-asset-hub.json

    # Configure parachain relay (penpal)
    jq \
        --arg k1 "$(address_for GatewayProxy)" \
        --arg k2 "$(address_for BeefyClient)" \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg channelID $PENPAL_CHANNEL_ID \
        --arg eth_gas_limit $eth_gas_limit \
        '
      .source.contracts.Gateway = $k1
    | .source.contracts.BeefyClient = $k2
    | .sink.contracts.Gateway = $k1
    | .source.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum.endpoint = $eth_endpoint_ws
    | .sink.ethereum."gas-limit" = $eth_gas_limit
    | .source."channel-id" = $channelID
    ' \
        config/parachain-relay.json >$output_dir/parachain-relay-penpal.json

    # Configure beacon relay
    jq \
        --arg beacon_endpoint_http $beacon_endpoint_http \
        --arg active_spec $active_spec \
        '
      .source.beacon.endpoint = $beacon_endpoint_http
    | .source.beacon.activeSpec = $active_spec
    ' \
        config/beacon-relay.json >$output_dir/beacon-relay.json

    # Configure execution relay for assethub
    jq \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg k1 "$(address_for GatewayProxy)" \
        --arg channelID $ASSET_HUB_CHANNEL_ID \
        '
      .source.ethereum.endpoint = $eth_endpoint_ws
    | .source.contracts.Gateway = $k1
    | .source."channel-id" = $channelID
    ' \
        config/execution-relay.json >$output_dir/execution-relay-asset-hub.json

    # Configure execution relay for penpal
    jq \
        --arg eth_endpoint_ws $eth_endpoint_ws \
        --arg k1 "$(address_for GatewayProxy)" \
        --arg channelID $PENPAL_CHANNEL_ID \
        '
              .source.ethereum.endpoint = $eth_endpoint_ws
            | .source.contracts.Gateway = $k1
            | .source."channel-id" = $channelID
            ' \
        config/execution-relay.json >$output_dir/execution-relay-penpal.json
}

start_relayer() {
    echo "Starting relay services"
    # Launch beefy relay
    (
        : >"$output_dir"/beefy-relay.log
        while :; do
            echo "Starting beefy relay at $(date)"
            "${relay_bin}" run beefy \
                --config "$output_dir/beefy-relay.json" \
                --ethereum.private-key $beefy_relay_eth_key \
                >>"$output_dir"/beefy-relay.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch parachain relay for bridgehub (primary governance)
    (
        : >"$output_dir"/parachain-relay-bridge-hub-01.log
        while :; do
            echo "Starting parachain-relay (primary governance) at $(date)"
            "${relay_bin}" run parachain \
                --config "$output_dir/parachain-relay-bridge-hub-01.json" \
                --ethereum.private-key $parachain_relay_eth_key \
                >>"$output_dir"/parachain-relay-bridge-hub-01.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch parachain relay for bridgehub (secondary governance)
    (
        : >"$output_dir"/parachain-relay-bridge-hub-02.log
        while :; do
            echo "Starting parachain-relay (secondary governance) at $(date)"
            "${relay_bin}" run parachain \
                --config "$output_dir/parachain-relay-bridge-hub-02.json" \
                --ethereum.private-key $parachain_relay_eth_key \
                >>"$output_dir"/parachain-relay-bridge-hub-02.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch parachain relay for assethub
    (
        : >"$output_dir"/parachain-relay-asset-hub.log
        while :; do
            echo "Starting parachain relay (asset-hub) at $(date)"
            "${relay_bin}" run parachain \
                --config "$output_dir/parachain-relay-asset-hub.json" \
                --ethereum.private-key $parachain_relay_eth_key \
                >>"$output_dir"/parachain-relay-asset-hub.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch parachain relay for parachain penpal
    (
        : >"$output_dir"/parachain-relay-penpal.log
        while :; do
            echo "Starting parachain-relay (penpal) at $(date)"
            "${relay_bin}" run parachain \
                --config "$output_dir/parachain-relay-penpal.json" \
                --ethereum.private-key $parachain_relay_eth_key \
                >>"$output_dir"/parachain-relay-penpal.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch beacon relay
    (
        : >"$output_dir"/beacon-relay.log
        while :; do
            echo "Starting beacon relay at $(date)"
            "${relay_bin}" run beacon \
                --config $output_dir/beacon-relay.json \
                --substrate.private-key "//BeaconRelay" \
                >>"$output_dir"/beacon-relay.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch execution relay for assethub
    (
        : >$output_dir/execution-relay-asset-hub.log
        while :; do
            echo "Starting execution relay (asset-hub) at $(date)"
            "${relay_bin}" run execution \
                --config $output_dir/execution-relay-asset-hub.json \
                --substrate.private-key "//ExecutionRelay" \
                >>"$output_dir"/execution-relay-asset-hub.log 2>&1 || true
            sleep 20
        done
    ) &

    # Launch execution relay for penpal
    (
        : >$output_dir/execution-relay-penpal.log
        while :; do
            echo "Starting execution relay (penpal) at $(date)"
            "${relay_bin}" run execution \
                --config $output_dir/execution-relay-penpal.json \
                --substrate.private-key "//ExecutionRelay" \
                >>"$output_dir"/execution-relay-penpal.log 2>&1 || true
            sleep 20
        done
    ) &
}

build_relayer() {
    echo "Building relayer"
    mage -d "$relay_dir" build
    cp $relay_bin "$output_bin_dir"
}

deploy_relayer() {
    check_tool && build_relayer && config_relayer && start_relayer
}

if [ -z "${from_start_services:-}" ]; then
    echo "start relayers only!"
    trap kill_all SIGINT SIGTERM EXIT
    deploy_relayer
    wait
fi
