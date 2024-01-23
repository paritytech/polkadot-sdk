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
  echo "Starting execution node"
  geth --$chain --datadir="$ethereum_data_dir" --authrpc.addr="127.0.0.1" --http.addr="0.0.0.0" --authrpc.vhosts "*" --http.corsdomain "*" --http --http.api web3,eth,net,engine,admin --ws --ws.api eth,net,web3 --authrpc.jwtsecret config/jwtsecret >"$output_dir/geth.log" 2>&1 &
  echo "Waiting for geth API to be ready"
  sleep 5
  echo "Starting beacon node"
  pushd $root_dir/lodestar
  ./lodestar beacon --dataDir="$ethereum_data_dir" --network=$chain --eth1=true --rest.namespace="*" --jwt-secret=$config_dir/jwtsecret --chain.archiveStateEpochFrequency=1 --checkpointSyncUrl=https://beaconstate-$chain.chainsafe.io >"$output_dir/lodestar.log" 2>&1 &
  popd
  wait
fi
