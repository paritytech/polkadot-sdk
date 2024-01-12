#!/bin/bash
#set -eu
set -x
shopt -s nullglob

trap "trap - SIGINT SIGTERM EXIT && kill -- -$$" SIGINT SIGTERM EXIT

# whether to use paths for zombienet+bridges tests container or for local testing
ZOMBIENET_DOCKER_PATHS=0
while [ $# -ne 0 ]
do
    arg="$1"
    case "$arg" in
        --docker)
            ZOMBIENET_DOCKER_PATHS=1
            ;;
    esac
    shift
done

# assuming that we'll be using native provide && all processes will be executing locally
# (we need absolute paths here, because they're used when scripts are called by zombienet from tmp folders)
export POLKADOT_SDK_FOLDER=`realpath $(dirname "$0")/../..`
export BRIDGE_TESTS_FOLDER=$POLKADOT_SDK_FOLDER/bridges/zombienet/tests

# set pathc to binaries
if [ "$ZOMBIENET_DOCKER_PATHS" -eq 1 ]; then
    export POLKADOT_BINARY_PATH=/usr/local/bin/polkadot
    export POLKADOT_PARACHAIN_BINARY_PATH=/usr/local/bin/polkadot-parachain
    export POLKADOT_PARACHAIN_BINARY_PATH_FOR_ASSET_HUB_ROCOCO=/usr/local/bin/polkadot-parachain
    export POLKADOT_PARACHAIN_BINARY_PATH_FOR_ASSET_HUB_WESTEND=/usr/local/bin/polkadot-parachain

    export SUBSTRATE_RELAY_PATH=/usr/local/bin/substrate-relay
    export ZOMBIENET_BINARY_PATH=/usr/local/bin/zombie
else
    export POLKADOT_BINARY_PATH=$POLKADOT_SDK_FOLDER/target/release/polkadot
    export POLKADOT_PARACHAIN_BINARY_PATH=$POLKADOT_SDK_FOLDER/target/release/polkadot-parachain
    export POLKADOT_PARACHAIN_BINARY_PATH_FOR_ASSET_HUB_ROCOCO=$POLKADOT_PARACHAIN_BINARY_PATH
    export POLKADOT_PARACHAIN_BINARY_PATH_FOR_ASSET_HUB_WESTEND=$POLKADOT_PARACHAIN_BINARY_PATH

    export SUBSTRATE_RELAY_PATH=~/local_bridge_testing/bin/substrate-relay
    export ZOMBIENET_BINARY_PATH=~/local_bridge_testing/bin/zombienet-linux
fi

# check if `wait` supports -p flag
if [ `printf "$BASH_VERSION\n5.1" | sort -V | head -n 1` = "5.1" ]; then IS_BASH_5_1=1; else IS_BASH_5_1=0; fi

# check if `wait` supports -p flag
if [ `printf "$BASH_VERSION\n5.1" | sort -V | head -n 1` = "5.1" ]; then IS_BASH_5_1=1; else IS_BASH_5_1=0; fi

# bridge configuration
export LANE_ID="00000002"

# tests configuration
ALL_TESTS_FOLDER=`mktemp -d /tmp/bridges-zombienet-tests.XXXXX`

function start_coproc() {
    local command=$1
    local name=$2
    local coproc_log=`mktemp -p $TEST_FOLDER`
    coproc COPROC {
        # otherwise zombienet uses some hardcoded paths
        unset RUN_IN_CONTAINER
        unset ZOMBIENET_IMAGE

        $command >$coproc_log 2>&1
    }
    TEST_COPROCS[$COPROC_PID, 0]=$name
    TEST_COPROCS[$COPROC_PID, 1]=$coproc_log
    echo "Spawned $name coprocess. StdOut + StdErr: $coproc_log"

    return $COPROC_PID
}

# execute every test from tests folder
TEST_INDEX=1
while true
do
    declare -A TEST_COPROCS
    TEST_COPROCS_COUNT=0
    TEST_PREFIX=$(printf "%04d" $TEST_INDEX)

    # it'll be used by the `sync-exit.sh` script
    export TEST_FOLDER=`mktemp -d -p $ALL_TESTS_FOLDER`

    # check if there are no more tests
    zndsl_files=($BRIDGE_TESTS_FOLDER/$TEST_PREFIX-*.zndsl)
    if [ ${#zndsl_files[@]} -eq 0 ]; then
        break
    fi

    # start relay
    if [ -f $BRIDGE_TESTS_FOLDER/$TEST_PREFIX-start-relay.sh ]; then
        start_coproc "${BRIDGE_TESTS_FOLDER}/${TEST_PREFIX}-start-relay.sh" "relay"
        RELAY_COPROC=$COPROC_PID
        ((TEST_COPROCS_COUNT++))
    fi
    # start tests
    for zndsl_file in "${zndsl_files[@]}"; do
        start_coproc "$ZOMBIENET_BINARY_PATH --provider native test $zndsl_file" "$zndsl_file"
        echo -n "1">>$TEST_FOLDER/exit-sync
        ((TEST_COPROCS_COUNT++))
    done
    # wait until all tests are completed
    relay_exited=0
    for n in `seq 1 $TEST_COPROCS_COUNT`; do
        if [ "$IS_BASH_5_1" -eq 1 ]; then
            wait -n -p COPROC_PID
            exit_code=$?
            coproc_name=${TEST_COPROCS[$COPROC_PID, 0]}
            coproc_log=${TEST_COPROCS[$COPROC_PID, 1]}
            coproc_stdout=$(cat $coproc_log)
            relay_exited=$(expr "${coproc_name}" == "relay")
        else
            wait -n
            exit_code=$?
            coproc_name="<unknown>"
            coproc_stdout="<unknown>"
        fi
        echo "Process $coproc_name has finished with exit code: $exit_code"

        # if exit code is not zero, exit
        if [ $exit_code -ne 0 ]; then
            echo "====================================================================="
            echo "=== Shutting down. Log of failed process below                    ==="
            echo "====================================================================="
            echo $coproc_stdout

            exit 1
        fi

        # if last test has exited, exit relay too
        if [ $n -eq $(($TEST_COPROCS_COUNT - 1)) ] && [ $relay_exited -eq 0 ]; then
            kill $RELAY_COPROC
            break
        fi
    done
    ((TEST_INDEX++))
done

echo "====================================================================="
echo "=== All tests have completed successfully                         ==="
echo "====================================================================="
