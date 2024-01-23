#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

if [ -z "${from_start_services:-}" ]; then
    echo "start nodes locally!"
    trap kill_all SIGINT SIGTERM EXIT
    # set true to clean db and start from a new checkpoint
    if [ "$reset_ethereum" == "true" ]; then
        echo "db reset!"
        rm -rf "$ethereum_data_dir"
    fi
    read -p "Chain: (goerli/sepolia/mainnet): " chain
    if [ "$chain" != "goerli" ] && [ "$chain" != "sepolia" ] && [ "$chain" != "mainnet" ]; then
        echo "chain type not allowed"
        exit
    fi
    pushd $root_dir/lodestar
    ./lodestar beacon --dataDir="$ethereum_data_dir" --network=$chain --execution.engineMock --eth1=false --rest.namespace="*" --chain.archiveStateEpochFrequency=1 --checkpointSyncUrl=https://beaconstate-$chain.chainsafe.io >"$output_dir/lodestar.log" 2>&1 &
    popd
    wait
fi
