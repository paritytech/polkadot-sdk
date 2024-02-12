#!/bin/bash

RELAY_LOG=`mktemp -p $TEST_FOLDER relay.XXXXX`

pushd $POLKADOT_SDK_PATH/cumulus/scripts
./bridges_rococo_westend.sh run-relay >$RELAY_LOG 2>&1&
popd
