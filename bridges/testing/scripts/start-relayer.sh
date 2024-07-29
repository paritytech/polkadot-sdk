#!/bin/bash

RELAY_LOG=`mktemp -p $TEST_FOLDER relay.XXXXX`

pushd $POLKADOT_SDK_PATH/bridges/testing/environments/rococo-westend
./bridges_rococo_westend.sh run-relay >$RELAY_LOG 2>&1&
popd
