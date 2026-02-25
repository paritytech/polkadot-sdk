#!/usr/bin/env bash

# This script executes a given zombienet test for the `native` provider.
# It is equivalent to running run-test-local-env-manager.sh for the `k8s` provider.

function run_test {
  cd "${OUTPUT_DIR}"
  for i in $(find ${OUTPUT_DIR} -name "${TEST_TO_RUN}"| head -1); do
    TEST_FOUND=1
    # in order to let native provider work properly we need
    # to unset ZOMBIENET_IMAGE, which controls 'inCI' internal flag.
    # ZOMBIENET_IMAGE not set && RUN_IN_CONTAINER=0 => inCI=false
    # Apparently inCI=true works properly only with k8s provider
    unset ZOMBIENET_IMAGE
    if [ -z "$ZOMBIE_BASE_DIR" ]; then
      ${ZOMBIE_COMMAND} -p native -c $CONCURRENCY test $i
    else
      ${ZOMBIE_COMMAND} -p native -c $CONCURRENCY -d $ZOMBIE_BASE_DIR -f test $i
    fi;
    EXIT_STATUS=$?
  done;
  if [[ $TEST_FOUND -lt 1 ]]; then
    EXIT_STATUS=1
  fi;
}

function create_isolated_dir {
  TS=$(date +%s)
  ISOLATED=${OUTPUT_DIR}/${TS}
  mkdir -p ${ISOLATED}
  OUTPUT_DIR="${ISOLATED}"
}

function copy_to_isolated {
  cd "${SCRIPT_PATH}"
  echo $(pwd)
  cp -r "${LOCAL_DIR}"/* "${OUTPUT_DIR}"
}

function rm_isolated_dir {
  echo "Removing ${OUTPUT_DIR}"
  rm -rf "${OUTPUT_DIR}"
}

function log {
  local lvl msg fmt
  lvl=$1 msg=$2
  fmt='+%Y-%m-%d %H:%M:%S'
  lg_date=$(date "${fmt}")
  if [[ "${lvl}" = "DIE" ]] ; then
    lvl="ERROR"
   echo -e "\n${lg_date} - ${lvl} - ${msg}"
   exit 1
  else
    echo -e "\n${lg_date} - ${lvl} - ${msg}"
  fi
}

set -x

SCRIPT_NAME="$0"
SCRIPT_PATH=$(dirname "$0")               # relative
SCRIPT_PATH=$(cd "${SCRIPT_PATH}" && pwd) # absolutized and normalized

ZOMBIE_COMMAND=zombie

EXIT_STATUS=0

# args
LOCAL_DIR="$1"
CONCURRENCY="$2"
TEST_TO_RUN="$3"
ZOMBIE_BASE_DIR="$4"

cd "${SCRIPT_PATH}"

OUTPUT_DIR="${SCRIPT_PATH}"

create_isolated_dir
copy_to_isolated
run_test
rm_isolated_dir

log INFO "Exit status is ${EXIT_STATUS}"
exit "${EXIT_STATUS}"
