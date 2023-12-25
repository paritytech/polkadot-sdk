#!/bin/bash
LOG_FILE=`mktemp -d /tmp/polkadot.XXXXX`
echo "Starting polkadot with arguments: $@. Log: $LOG_FILE"
/usr/local/bin/polkadot "$@" 2>&1 | tee $LOG_FILE/log
echo "Stopping polkadot with arguments: $@"
