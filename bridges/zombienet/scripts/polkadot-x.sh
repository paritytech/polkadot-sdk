#!/bin/bash
LOG_FILE=`mktemp -d /tmp/polkadot.XXXXX`
echo "Starting polkadot with arguments: $@. Log: $LOG_FILE" >$LOG_FILE/log
/usr/local/bin/polkadot "$@" 2>&1 | tee -a $LOG_FILE/log
echo "Stopping polkadot with arguments: $@" >>$LOG_FILE/log
