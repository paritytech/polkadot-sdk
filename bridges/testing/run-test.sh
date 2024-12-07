#!/bin/bash

set -e

trap 'kill -9 -$$ || echo "Environment already teared down"' SIGINT SIGTERM EXIT

test=$1
shift

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

export POLKADOT_SDK_PATH=`realpath ${BASH_SOURCE%/*}/../..`
export FRAMEWORK_PATH=`realpath ${BASH_SOURCE%/*}/framework`

# set path to binaries
if [ "$ZOMBIENET_DOCKER_PATHS" -eq 1 ]; then
    # otherwise zombienet uses some hardcoded paths
    unset RUN_IN_CONTAINER
    unset ZOMBIENET_IMAGE

    export POLKADOT_BINARY=/usr/local/bin/polkadot
    export POLKADOT_PARACHAIN_BINARY=/usr/local/bin/polkadot-parachain

    export ZOMBIENET_BINARY=/usr/local/bin/zombie
    export SUBSTRATE_RELAY_BINARY=/usr/local/bin/substrate-relay
else
    export POLKADOT_BINARY=$POLKADOT_SDK_PATH/target/release/polkadot
    export POLKADOT_PARACHAIN_BINARY=$POLKADOT_SDK_PATH/target/release/polkadot-parachain

    export ZOMBIENET_BINARY=~/local_bridge_testing/bin/zombienet-linux-x64
    export SUBSTRATE_RELAY_BINARY=~/local_bridge_testing/bin/substrate-relay
fi

export TEST_DIR=`mktemp -d /tmp/bridges-tests-run-XXXXX`
echo -e "Test folder: $TEST_DIR\n"

${BASH_SOURCE%/*}/tests/$test/run.sh
