#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$0 <url> <seed> <wasm> <genesis> <parachain-id> <types>"
    exit 1
}

url=$1
seed=$2
wasm=$3
genesis=$4
parachain_id=$5
types=$6 # we can remove this once parachain types are included in polkadot-js-api

[ -z "$url" ] && usage
[ -z "$seed" ] && usage
[ -z "$wasm" ] && usage
[ -z "$types" ] && usage
[ -z "$genesis" ] && usage
[ -z "$parachain_id" ] && usage
if ! [ -r "$wasm" ]; then
    echo "Could not read: $wasm"
    exit 1
fi
if ! [ -r "$types" ]; then
    echo "Could not read: $types"
    exit 1
fi

if ! which polkadot-js-api &> /dev/null; then
    echo 'command `polkadot-js-api` not in PATH'
    echo "npm install -g @polkadot/api-cli@beta"
    exit 1
fi

set -e -x

test -f "$seed" && seed="$(cat "$seed")"

wasm=$(cat $wasm)

polkadot-js-api \
    --ws "${url?}" \
    --sudo \
    --seed "${seed?}" \
    --types "${types?}" \
    tx.parasSudoWrapper.sudoScheduleParaInitialize \
        "${parachain_id?}" \
        "{ \"genesisHead\":\"${genesis?}\", \"validationCode\":\"${wasm?}\", \"parachain\": true }" \
