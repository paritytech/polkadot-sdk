#!/bin/bash

set -e

source "${BASH_SOURCE%/*}/../../utils/common.sh"
source "${BASH_SOURCE%/*}/../../utils/zombienet.sh"

polkadot_dir=$1
kusama_dir=$2
__relayer_pid=$3

logs_dir=$TEST_DIR/logs
helper_script="${BASH_SOURCE%/*}/helper.sh"

relayer_log=$logs_dir/relayer.log
echo -e "Starting polkadot-kusama relayer. Logs available at: $relayer_log\n"
start_background_process "$helper_script run-relay" $relayer_log relayer_pid

run_zndsl ${BASH_SOURCE%/*}/polkadot.zndsl $polkadot_dir
run_zndsl ${BASH_SOURCE%/*}/kusama.zndsl $kusama_dir

eval $__relayer_pid="'$relayer_pid'"

