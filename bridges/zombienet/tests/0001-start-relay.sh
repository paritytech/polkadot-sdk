#!/bin/bash

pushd $POLKADOT_SDK_FOLDER/cumulus/scripts
./bridges_rococo_wococo.sh run-relay
popd
