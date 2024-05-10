#!/bin/bash

if [ $1 == "auto-log" ]; then
    shift # ignore "auto-log"
    log_name=$1
    $ENV_PATH/bridges_rococo_westend.sh "$@" >$TEST_DIR/logs/$log_name.log
else
    $ENV_PATH/bridges_rococo_westend.sh "$@"
fi
