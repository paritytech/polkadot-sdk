#!/usr/bin/env bash

# this script runs the polkadot-parachain after fetching
# appropriate bootnode IDs
#
# this is _not_ a general-purpose script; it is closely tied to the
# root docker-compose.yml

set -e -o pipefail

ctpc="/usr/bin/polkadot-parachain"

if [ ! -x "$ctpc" ]; then
    echo "FATAL: $ctpc does not exist or is not executable"
    exit 1
fi

# name the variable with the incoming args so it isn't overwritten later by function calls
args=( "$@" )

alice="172.28.1.1"
bob="172.28.1.2"
p2p_port="30333"
rpc_port="9933"


get_id () {
    node="$1"
    /wait-for-it.sh "$node:$rpc_port" -t 10 -s -- \
        curl -sS \
            -H 'Content-Type: application/json' \
            --data '{"id":1,"jsonrpc":"2.0","method":"system_networkState"}' \
            "$node:$rpc_port" |\
    jq -r '.result.peerId'
}

bootnode () {
    node="$1"
    id=$(get_id "$node")
    if [ -z "$id" ]; then
        echo >&2 "failed to get id for $node"
        exit 1
    fi
    echo "/ip4/$node/tcp/$p2p_port/p2p/$id"
}

args+=( "--" "--bootnodes=$(bootnode "$alice")" "--bootnodes=$(bootnode "$bob")" )

set -x
"$ctpc" "${args[@]}"
