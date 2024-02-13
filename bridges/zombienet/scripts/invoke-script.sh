#!/bin/bash

INVOKE_LOG=`mktemp -p $TEST_FOLDER invoke.XXXXX`

pushd $POLKADOT_SDK_PATH/cumulus/scripts
./bridges_rococo_westend.sh $1 >$INVOKE_LOG 2>&1
popd
