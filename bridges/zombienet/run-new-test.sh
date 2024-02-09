#!/bin/bash

set -e

trap "trap - SIGTERM && kill -9 -$$" SIGINT SIGTERM EXIT

test=$1

export POLKADOT_SDK_PATH=`realpath ${BASH_SOURCE%/*}/../..`

export POLKADOT_BINARY=$POLKADOT_SDK_PATH/target/release/polkadot
export POLKADOT_PARACHAIN_BINARY=$POLKADOT_SDK_PATH/target/release/polkadot-parachain

export ZOMBIENET_BINARY=~/local_bridge_testing/bin/zombienet-linux-x64
export SUBSTRATE_RELAY_BINARY=~/local_bridge_testing/bin/substrate-relay

export TEST_DIR=`mktemp -d /tmp/bridges-tests-run-XXXXX`
echo -e "Test folder: $TEST_DIR\n"

${BASH_SOURCE%/*}/tests/$test/run.sh

kill -9 -$$