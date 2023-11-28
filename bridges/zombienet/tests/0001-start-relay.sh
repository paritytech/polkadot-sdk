#!/bin/bash

pushd $POLKADOT_SDK_FOLDER/cumulus/scripts
./bridges_rococo_westend.sh run-relay
popd
