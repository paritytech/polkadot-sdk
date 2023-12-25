#!/bin/bash
LOG_FILE=`mktemp -d /tmp/polkadot-parachain.XXXXX`
echo "Starting polkadot-parachain with arguments: $@. Log: $LOG_FILE"
/usr/local/bin/polkadot-parachain "$@" 2>&1 | tee $LOG_FILE/log
echo "Stopping polkadot-parachain with arguments: $@"
