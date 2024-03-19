#!/usr/bin/env bash

# Example usage to:
#
# - Use the `polkadot-parachain` binary;
# - Use `rococo` as the parent Relay Chain;
# - Generate `ParaId`s from 1,300 to 1,370, inclusive;
# - Set the Sudo key to `GZ9YSgtib4kEMxWcpWfnXa1cnrumspTCTZSaNWWmMkJbWqW`;
# - Set `compute`, `storage`, and `trash_data_count` set to 50%, 131%, and 5,120, respectively;
# - And save the results in `output-dir`.
#
# ./scripts/create_glutton_spec.sh ./target/release/polkadot-parachain rococo 1300 1370 GZ9YSgtib4kEMxWcpWfnXa1cnrumspTCTZSaNWWmMkJbWqW 500000000 1310000000 5120 output-dir

usage() {
    echo Usage:
    echo "$0 <binary path> <relay chain> <from parachain id> <to parachain id> <sudo key> <compute> <storage> <trash_data_count> <output dir>"
    exit 1
}

set -e

if ! command -v jq >/dev/null 2>&1; then
    echo "'jq' is not installed, please install. Exiting..."
    exit 1
fi

binary_path=$1
relay_chain=$2
from_para_id=$3
to_para_id=$4
sudo=$5
compute=$6
storage=$7
trash_data_count=$8
output_dir=$9

[ -z "$binary_path" ] && usage
[ -z "$relay_chain" ] && usage
[ -z "$from_para_id" ] && usage
[ -z "$to_para_id" ] && usage
[ -z "$sudo" ] && usage
[ -z "$compute" ] && usage
[ -z "$storage" ] && usage
[ -z "$trash_data_count" ] && usage
[ -z "$output_dir" ] && usage


for (( para_id=$from_para_id; para_id<=$to_para_id; para_id++ )); do
    echo "Building chain specs for parachain $para_id"

    # create dir to store parachain generated files
    output_para_dir="$output_dir/glutton-$relay_chain-$para_id"
    if [ ! -d "$output_para_dir" ]; then
        mkdir $output_para_dir
    fi

    # build the chain spec we'll manipulate
    $binary_path build-spec --disable-default-bootnode --chain "glutton-westend-genesis-$para_id" > "$output_para_dir/plain-glutton-$relay_chain-$para_id-spec.json"

    id="glutton-$relay_chain-$para_id"
    protocol_id="glutton-$relay_chain-$para_id"

    # replace the runtime in the spec with the given runtime and set some values to production
    cat "$output_para_dir/plain-glutton-$relay_chain-$para_id-spec.json" \
        | jq --arg id $id '.id = $id' \
        | jq --arg protocol_id $protocol_id '.protocolId = $protocol_id' \
        | jq --arg relay_chain $relay_chain '.relay_chain = $relay_chain' \
        | jq --argjson para_id $para_id '.para_id = $para_id' \
        | jq --arg sudo $sudo '.genesis.runtime.sudo.key = $sudo' \
        | jq --argjson para_id $para_id '.genesis.runtime.parachainInfo.parachainId = $para_id' \
        | jq --arg compute $compute '.genesis.runtime.glutton.compute = $compute' \
        | jq --arg storage $storage '.genesis.runtime.glutton.storage = $storage' \
        | jq --argjson trash_data_count $trash_data_count '.genesis.runtime.glutton.trashDataCount = $trash_data_count' \
        > $output_para_dir/glutton-$relay_chain-$para_id-spec.json

    # build a raw spec
    $binary_path build-spec --disable-default-bootnode --chain "$output_para_dir/glutton-$relay_chain-$para_id-spec.json" --raw > "$output_para_dir/glutton-$relay_chain-$para_id-raw-spec.json"

    # build genesis data
    $binary_path export-genesis-state --chain "$output_para_dir/glutton-$relay_chain-$para_id-raw-spec.json" > "$output_para_dir/glutton-$relay_chain-$para_id-head-data"

    # build genesis wasm
    $binary_path export-genesis-wasm --chain "$output_para_dir/glutton-$relay_chain-$para_id-raw-spec.json" > "$output_para_dir/glutton-$relay_chain-$para_id-validation-code"

    rm "$output_para_dir/plain-glutton-$relay_chain-$para_id-spec.json"
done
