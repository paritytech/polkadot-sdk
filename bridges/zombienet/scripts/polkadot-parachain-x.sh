#!/bin/bash
LOG_FILE=`mktemp -d /tmp/polkadot-parachain.XXXXX`
echo "Starting polkadot-parachain with arguments: $@"
/usr/local/bin/polkadot-parachain "$@" 2>&1 | tee $LOG_FILE
echo "Stopping polkadot-parachain with arguments: $@"
