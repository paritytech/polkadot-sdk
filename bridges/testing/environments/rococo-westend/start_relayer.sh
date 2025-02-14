#!/bin/bash

set -e

source "$FRAMEWORK_PATH/utils/common.sh"
source "$FRAMEWORK_PATH/utils/zombienet.sh"

rococo_dir=$1
westend_dir=$2
__finality_relayer_pid=$3
__parachains_relayer_pid=$4
__messages_relayer_pid=$5

logs_dir=$TEST_DIR/logs
helper_script="${BASH_SOURCE%/*}/helper.sh"

# start finality relayer
finality_relayer_log=$logs_dir/relayer_finality.log
echo -e "Starting rococo-westend finality relayer. Logs available at: $finality_relayer_log\n"
start_background_process "$helper_script run-finality-relay" $finality_relayer_log finality_relayer_pid

# start parachains relayer
parachains_relayer_log=$logs_dir/relayer_parachains.log
echo -e "Starting rococo-westend parachains relayer. Logs available at: $parachains_relayer_log\n"
start_background_process "$helper_script run-parachains-relay" $parachains_relayer_log parachains_relayer_pid

# start messages relayer
messages_relayer_log=$logs_dir/relayer_messages.log
echo -e "Starting rococo-westend messages relayer. Logs available at: $messages_relayer_log\n"
start_background_process "$helper_script run-messages-relay" $messages_relayer_log messages_relayer_pid

run_zndsl ${BASH_SOURCE%/*}/rococo-bridge.zndsl $rococo_dir
run_zndsl ${BASH_SOURCE%/*}/westend-bridge.zndsl $westend_dir

eval $__finality_relayer_pid="'$finality_relayer_pid'"
eval $__parachains_relayer_pid="'$parachains_relayer_pid'"
eval $__messages_relayer_pid="'$messages_relayer_pid'"
