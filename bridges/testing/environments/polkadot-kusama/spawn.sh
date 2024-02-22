#!/bin/bash

set -e

trap "trap - SIGTERM && kill -9 -$$" SIGINT SIGTERM EXIT

source "${BASH_SOURCE%/*}/../../utils/zombienet.sh"

export ENV_PATH=${BASH_SOURCE%/*}

logs_dir=$TEST_DIR/logs
helper_script="${BASH_SOURCE%/*}/helper.sh"

polkadot_def=${BASH_SOURCE%/*}/bridge_hub_polkadot_local_network.toml
start_zombienet $TEST_DIR $polkadot_def polkadot_dir polkadot_pid
echo

kusama_def=${BASH_SOURCE%/*}/bridge_hub_kusama_local_network.toml
start_zombienet $TEST_DIR $kusama_def kusama_dir kusama_pid
echo

polkadot_init_log=$logs_dir/polkadot-init.log
echo -e "Setting up the polkadot side of the bridge. Logs available at: $polkadot_init_log\n"

kusama_init_log=$logs_dir/kusama-init.log
echo -e "Setting up the kusama side of the bridge. Logs available at: $kusama_init_log\n"

$helper_script init-asset-hub-polkadot-local >> $polkadot_init_log 2>&1 &
polkadot_init_pid=$!
$helper_script init-asset-hub-kusama-local >> $kusama_init_log 2>&1 &
kusama_init_pid=$!
wait -n $polkadot_init_pid $kusama_init_pid


$helper_script init-bridge-hub-polkadot-local >> $polkadot_init_log 2>&1 &
polkadot_init_pid=$!
$helper_script init-bridge-hub-kusama-local >> $kusama_init_log 2>&1 &
kusama_init_pid=$!
wait -n $polkadot_init_pid $kusama_init_pid

run_zndsl ${BASH_SOURCE%/*}/polkadot-init.zndsl $polkadot_dir
run_zndsl ${BASH_SOURCE%/*}/kusama-init.zndsl $kusama_dir

${BASH_SOURCE%/*}/start_relayer.sh $polkadot_dir $kusama_dir relayer_pid

echo $polkadot_dir > $TEST_DIR/polkadot.env
echo $kusama_dir > $TEST_DIR/kusama.env
echo

wait -n $polkadot_pid $kusama_pid $relayer_pid
kill -9 -$$
