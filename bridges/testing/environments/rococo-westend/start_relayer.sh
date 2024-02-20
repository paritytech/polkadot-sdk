#!/bin/bash

set -e

source "${BASH_SOURCE%/*}/../../utils/common.sh"
source "${BASH_SOURCE%/*}/../../utils/zombienet.sh"

rococo_dir=$1
westend_dir=$2
__relayer_pid=$3

logs_dir=$TEST_DIR/logs
helper_script="${BASH_SOURCE%/*}/helper.sh"

relayer_log=$logs_dir/relayer.log
echo -e "Starting rococo-westend relayer. Logs available at: $relayer_log\n"
start_background_process "$helper_script run-relay" $relayer_log relayer_pid

run_zndsl ${BASH_SOURCE%/*}/rococo.zndsl $rococo_dir
run_zndsl ${BASH_SOURCE%/*}/westend.zndsl $westend_dir

eval $__relayer_pid="'$relayer_pid'"

