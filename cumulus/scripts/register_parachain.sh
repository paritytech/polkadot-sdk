#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$0 <url> <seed> <wasm> <genesis> <parachain-id> <tokens> <account>"
    exit 1
}

url=$1
seed=$2
wasm=$3
genesis=$4
parachain_id=$5
tokens=$6
account=$7

[ -z "$url" ] && usage
[ -z "$seed" ] && usage
[ -z "$wasm" ] && usage
[ -z "$genesis" ] && usage
[ -z "$parachain_id" ] && usage
[ -z "$tokens" ] && usage
[ -z "$account" ] && usage
if ! [ -r "$wasm" ]; then
    echo "Could not read: $wasm"
    exit 1
fi

if ! which polkadot-js-api &> /dev/null; then
    echo 'command `polkadot-js-api` not in PATH'
    echo "npm install -g @polkadot/api-cli"
    exit 1
fi

set -e -x

test -f "$seed" && seed="$(cat "$seed")"

polkadot-js-api \
    --ws "${url?}" \
    --sudo \
    --seed "${seed?}" \
    tx.registrar.registerPara \
        "${parachain_id?}" \
        '{"scheduling":"Always"}' \
        @"${wasm?}" \
        "${genesis?}"

polkadot-js-api \
    --ws "${url?}" \
    --sudo \
    --seed "${seed?}" \
    tx.balances.setBalance \
        "${account?}" \
        $((tokens * 10 ** 12)) \
        0
