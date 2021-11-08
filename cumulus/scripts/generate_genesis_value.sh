#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$0 <chain-id>"
    exit 1
}

chain_id=$1

[ -z "$chain_id" ] && usage

pushd generate_genesis_values
yarn
popd

node generate_genesis_values ../polkadot-parachains/res/$chain_id.json ../polkadot-parachains/res/${chain_id}_genesis_values.json

pushd scale_encode_genesis
yarn
popd
node scale_encode_genesis ../polkadot-parachains/res/${chain_id}_genesis_values.json ${chain_id}_genesis_values.txt
