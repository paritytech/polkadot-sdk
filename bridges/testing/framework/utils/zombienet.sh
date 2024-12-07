#!/bin/bash

source "${BASH_SOURCE%/*}/common.sh"

function start_zombienet() {
    local test_dir=$1
    local definition_path=$2
    local __zombienet_dir=$3
    local __zombienet_pid=$4

    local zombienet_name=`basename $definition_path .toml`
    local zombienet_dir=$test_dir/$zombienet_name
    eval $__zombienet_dir="'$zombienet_dir'"
    mkdir -p $zombienet_dir
    rm -rf $zombienet_dir

    local logs_dir=$test_dir/logs
    mkdir -p $logs_dir
    local zombienet_log=$logs_dir/$zombienet_name.log

    echo "Starting $zombienet_name zombienet. Logs available at: $zombienet_log"
    start_background_process \
        "$ZOMBIENET_BINARY spawn --dir $zombienet_dir --provider native $definition_path" \
        "$zombienet_log" zombienet_pid

    ensure_process_file $zombienet_pid "$zombienet_dir/zombie.json" 180
    echo "$zombienet_name zombienet started successfully"

    eval $__zombienet_pid="'$zombienet_pid'"
}

function run_zndsl() {
    local zndsl_file=$1
    local zombienet_dir=$2

    echo "Running $zndsl_file."
    $ZOMBIENET_BINARY test --dir $zombienet_dir --provider native $zndsl_file $zombienet_dir/zombie.json
    echo
}
