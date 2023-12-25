#!/bin/bash
LOG_FILE=`mktemp -d /tmp/polkadot-parachain.XXXXX`
echo "Starting polkadot-parachain with arguments: $@. Log: $LOG_FILE" >$LOG_FILE/log
/usr/local/bin/polkadot-parachain "$@" 2>&1 | tee -a $LOG_FILE/log
echo "Stopping polkadot-parachain with arguments: $@" >>$LOG_FILE/log
