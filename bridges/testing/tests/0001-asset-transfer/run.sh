#!/bin/bash

set -e

source "${BASH_SOURCE%/*}/../../utils/common.sh"
source "${BASH_SOURCE%/*}/../../utils/zombienet.sh"

${BASH_SOURCE%/*}/../../environments/rococo-westend/spawn.sh --init --start-relayer &
env_pid=$!

ensure_process_file $env_pid $TEST_DIR/rococo.env 400
rococo_dir=`cat $TEST_DIR/rococo.env`
echo

ensure_process_file $env_pid $TEST_DIR/westend.env 180
westend_dir=`cat $TEST_DIR/westend.env`
echo

run_zndsl ${BASH_SOURCE%/*}/roc-reaches-westend.zndsl $westend_dir
run_zndsl ${BASH_SOURCE%/*}/wnd-reaches-rococo.zndsl $rococo_dir

run_zndsl ${BASH_SOURCE%/*}/wroc-reaches-rococo.zndsl $rococo_dir
run_zndsl ${BASH_SOURCE%/*}/wwnd-reaches-westend.zndsl $westend_dir
