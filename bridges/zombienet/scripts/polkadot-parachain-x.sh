#!/bin/bash

ARGS="$@"
LOG_FILE=`mktemp /tmp/polkadot-parachain.XXXXX`
BUILD_SPEC=0
while test $# -gt 0
do
    case "$1" in
        build-spec)
            BUILD_SPEC=1
            ;;
    esac
    shift
done

if [ $BUILD_SPEC -eq 1 ]; then
	/usr/local/bin/polkadot-parachain $ARGS
else
	echo "Starting polkadot-parachain with arguments: $ARGS. Log: $LOG_FILE" >$LOG_FILE
	/usr/local/bin/polkadot-parachain $ARGS 2>&1 | tee -a $LOG_FILE
	echo "Stopping polkadot-parachain with arguments: $ARGS" >>$LOG_FILE
fi



