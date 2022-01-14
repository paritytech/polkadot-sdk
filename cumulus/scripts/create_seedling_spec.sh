#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$0 <srtool compressed runtime path> <name> <id> <chain type> <bootnodes> <relay chain> <parachain id> <sudo key>"
    exit 1
}

set -e

runtime_path=$1
name=$2
id="seedling-$3"
chain_type=$4
bootnodes=$5
relay_chain=$6
para_id=$7
sudo=$8

[ -z "$runtime_path" ] && usage
[ -z "$name" ] && usage
[ -z "$id" ] && usage
[ -z "$chain_type" ] && usage
[ -z "$bootnodes" ] && usage
[ -z "$relay_chain" ] && usage
[ -z "$para_$id" ] && usage
[ -z "$sudo" ] && usage

binary="./target/release/polkadot-collator"

# build the chain spec we'll manipulate
$binary build-spec --disable-default-bootnode --chain seedling > seedling-spec-plain.json

# convert runtime to hex
cat $runtime_path | od -A n -v -t x1 |  tr -d ' \n' > seedling-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
cat seedling-spec-plain.json | jq --rawfile code seedling-hex.txt '.genesis.runtime.system.code = ("0x" + $code)' \
    | jq --arg name $name '.name = $name' \
    | jq --arg id $id '.id = $id' \
    | jq --arg chain_type $chain_type '.chainType = $chain_type' \
    | jq --argjson bootnodes $bootnodes '.bootNodes = $bootnodes' \
    | jq --arg relay_chain $relay_chain '.relay_chain = $relay_chain' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --arg sudo $sudo '.genesis.runtime.sudo.key = $sudo' \
    | jq --argjson para_id $para_id '.genesis.runtime.parachainInfo.parachainId = $para_id' \
    > edited-seedling-plain.json

# build a raw spec
$binary build-spec --disable-default-bootnode --chain edited-seedling-plain.json --raw > seedling-spec-raw.json

# build genesis data
$binary export-genesis-state --parachain-id=$para_id --chain seedling-spec-raw.json > seedling-head-data

# build genesis wasm
$binary export-genesis-wasm --chain seedling-spec-raw.json > seedling-wasm
