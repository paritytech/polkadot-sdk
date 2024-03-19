#!/bin/bash

function start_background_process() {
    local command=$1
    local log_file=$2
    local __pid=$3

    $command > $log_file 2>&1 &
    eval $__pid="'$!'"
}

function wait_for_process_file() {
    local pid=$1
    local file=$2
    local timeout=$3
    local __found=$4

    local time=0
    until [ -e $file ]; do
      if ! kill -0 $pid; then
        echo "Process finished unsuccessfully"
        return
      fi
      if (( time++ >= timeout )); then
        echo "Timeout waiting for file $file: $timeout seconds"
        eval $__found=0
        return
      fi
      sleep 1
    done

    echo "File $file found after $time seconds"
    eval $__found=1
}

function ensure_process_file() {
    local pid=$1
    local file=$2
    local timeout=$3

    wait_for_process_file $pid $file $timeout file_found
    if [ "$file_found" != "1" ]; then
      exit 1
    fi
}
