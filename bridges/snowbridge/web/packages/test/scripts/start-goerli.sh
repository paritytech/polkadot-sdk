#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

deploy_goerli()
{
    echo "Starting execution node"
    geth --goerli --datadir="$ethereum_data_dir" --authrpc.addr="127.0.0.1" --http.addr="0.0.0.0" --authrpc.vhosts "*" --http.corsdomain "*" --http --http.api web3,eth,net,engine,admin --ws --ws.api eth,net,web3 --authrpc.jwtsecret config/jwtsecret > "$output_dir/geth.log" 2>&1 &
    echo "Waiting for geth API to be ready"
    sleep 3
    echo "Starting beacon node"
    # explicit config max-old-space-size or will be oom
    node --max-old-space-size=4096 ../../node_modules/.pnpm/@chainsafe+lodestar@1.8.0_c-kzg@1.1.3_fastify@3.15.1/node_modules/@chainsafe/lodestar/lib/index.js beacon --dataDir="$ethereum_data_dir" --network=goerli --eth1=true --rest.namespace="*" --jwt-secret=./config/jwtsecret --checkpointSyncUrl=https://sync-goerli.beaconcha.in > "$output_dir/lodestar.log" 2>&1 &
    echo "Waiting for beacon node to sync from checkpoint"
    sleep 3
    echo "Ethereum started!"
}

if [ -z "${from_start_services:-}" ]; then
    echo "start goerli locally!"
    trap kill_all SIGINT SIGTERM EXIT
    # change to true to rm data dir and start from a new checkpoint
    reinitialize="false"
    if [ "$reinitialize" == "true" ]; then
      rm -rf "$ethereum_data_dir"
    fi
    deploy_goerli
    wait
fi
