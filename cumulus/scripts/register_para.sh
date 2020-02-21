#!/usr/bin/env bash

set -e -o pipefail

sizeof () {
    stat --printf="%s" "$1"
}

wait_for_file () {
    # Wait for a file to have a stable, non-zero size.
    # Takes at least 0.2 seconds per run, but there's no upper bound if the
    # file grows continuously. If the file doesn't exist, or stably has 0 size,
    # this will take up to 10 seconds by default; this limit can be adjusted by
    # the second input parameter.
    path="$1"
    limit="$2"
    if [ -z "$limit" ]; then
        limit=10
    fi
    count=0
    while [ "$count" -lt "$limit" ]; do
        if [ -s "$path" ]; then
            echo "$path found after $count seconds"
            # now ensure that the file size is stable: it's not still being written
            oldsize=0
            size="$(sizeof "$path")"
            while [ "$oldsize" -ne "$size" ]; do
                sleep 0.2
                oldsize="$size"
                size="$(sizeof "$path")"
            done
            return
        fi
        count=$((count+1))
        sleep 1
    done
    echo "$path not found after $count seconds"
    exit 1
}

wait_for_file /runtime/cumulus_test_parachain_runtime.compact.wasm
wait_for_file /genesis/genesis-state

# this is now straightforward: just send the sudo'd tx to the alice node,
# as soon as the node is ready to receive connections
/wait-for-it.sh 172.28.1.1:9944 \
    --strict \
    --timeout=10 \
    -- \
    polkadot-js-api \
        --ws ws://172.28.1.1:9944 \
        --sudo \
        --seed "//Alice" \
        tx.registrar.registerPara \
            100 \
            '{"scheduling":"Always"}' \
            @/runtime/cumulus_test_parachain_runtime.compact.wasm \
            "$(cat /genesis/genesis-state)"
