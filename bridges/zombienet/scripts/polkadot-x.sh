#!/bin/bash
LOG_FILE=`mktemp -d /tmp/polkadot.XXXXX`
echo "Starting polkadot with arguments: $@"
/usr/local/bin/polkadot "$@" 2>&1 | tee $LOG_FILE
echo "Stopping polkadot with arguments: $@"
