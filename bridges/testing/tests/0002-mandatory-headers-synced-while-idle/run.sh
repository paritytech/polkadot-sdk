#!/bin/bash

set -e

source "${BASH_SOURCE%/*}/../../framework/utils/common.sh"
source "${BASH_SOURCE%/*}/../../framework/utils/zombienet.sh"

export ENV_PATH=`realpath ${BASH_SOURCE%/*}/../../environments/rococo-westend`

$ENV_PATH/spawn.sh &
env_pid=$!

ensure_process_file $env_pid $TEST_DIR/rococo.env 600
rococo_dir=`cat $TEST_DIR/rococo.env`
echo

ensure_process_file $env_pid $TEST_DIR/westend.env 300
westend_dir=`cat $TEST_DIR/westend.env`
echo

# Sleep for some time before starting the relayer. We want to sleep for at least 1 session,
# which is expected to be 60 seconds for the test environment.
echo -e "Sleeping 90s before starting relayer ...\n"
sleep 90
${BASH_SOURCE%/*}/../../environments/rococo-westend/start_relayer.sh $rococo_dir $westend_dir relayer_pid

# Sometimes the relayer syncs multiple parachain heads in the beginning leading to test failures.
# See issue: https://github.com/paritytech/parity-bridges-common/issues/2838.
# TODO: Remove this sleep after the issue is fixed.
echo -e "Sleeping 180s before runing the tests ...\n"
sleep 180

run_zndsl ${BASH_SOURCE%/*}/rococo-to-westend.zndsl $westend_dir
run_zndsl ${BASH_SOURCE%/*}/westend-to-rococo.zndsl $rococo_dir

