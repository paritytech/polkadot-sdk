#!/bin/bash

set -e

trap "trap - SIGTERM && kill -9 -$$" SIGINT SIGTERM EXIT

source "${BASH_SOURCE%/*}/../../utils/common.sh"
source "${BASH_SOURCE%/*}/../../utils/zombienet.sh"

logs_dir=$TEST_DIR/logs
helper_script="${BASH_SOURCE%/*}/helper.sh"

rococo_def=$POLKADOT_SDK_PATH/cumulus/zombienet/bridge-hubs/bridge_hub_rococo_local_network.toml
start_zombienet $TEST_DIR $rococo_def rococo_dir rococo_pid
echo

rococo_init_log=$logs_dir/rococo-init.log
echo -e "Setting up the rococo side of the bridge. Logs available at: $rococo_init_log\n"
$helper_script init-asset-hub-rococo-local >> $rococo_init_log 2>&1
$helper_script init-bridge-hub-rococo-local >> $rococo_init_log 2>&1
echo

westend_def=$POLKADOT_SDK_PATH/cumulus/zombienet/bridge-hubs/bridge_hub_westend_local_network.toml
start_zombienet $TEST_DIR $westend_def westend_dir westend_pid
echo

westend_init_log=$logs_dir/westend-init.log
echo -e "Setting up the westend side of the bridge. Logs available at: $westend_init_log\n"
$helper_script init-asset-hub-westend-local >> $westend_init_log 2>&1
$helper_script init-bridge-hub-westend-local >> $westend_init_log 2>&1
echo

relay_log=$logs_dir/relay.log
echo -e "Starting rococo-westend relay. Logs available at: $relay_log\n"
start_background_process "$helper_script run-relay" $relay_log relay_pid

run_zndsl ${BASH_SOURCE%/*}/rococo.zndsl $rococo_dir
echo $rococo_dir > $TEST_DIR/rococo.env
echo

run_zndsl ${BASH_SOURCE%/*}/westend.zndsl $westend_dir
echo $westend_dir > $TEST_DIR/westend.env
echo

wait -n $rococo_pid $westend_pid $relay_pid
kill -9 -$$
