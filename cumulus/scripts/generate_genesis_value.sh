#!/usr/bin/env bash

# Call from the root of the repo as:
# ./scripts/generate_genesis_value.sh <chain-id> [rpc endpoint]
usage() {
    echo Usage:
    echo "$0 <chain-id> [rpc endpoint]"
    exit 1
}

chain_spec_summary() {
    if [ -f $chain_spec ]; then
        echo -e "ℹ️ Using chain specs from" $chain_spec
        echo -e " - name        :" $(jq  -r .name $chain_spec)
        echo -e " - id          :" $(jq  -r .id $chain_spec)
        echo -e " - type        :" $(jq  -r .chainType $chain_spec)
        echo -e " - decimals    :" $(jq  -r .properties.tokenDecimals $chain_spec)
        echo -e " - symbol      :" $(jq  -r .properties.tokenSymbol $chain_spec)
        echo -e " - relay_chain :" $(jq  -r .relay_chain $chain_spec)
        echo -e " - para_id     :" $(jq  -r .para_id $chain_spec)
        echo -e " - bootNodes   :" $(jq  '.bootNodes | length' $chain_spec)
        echo
    else
        echo "❌ Chain specs not found from" $chain_spec
        exit 1
    fi
}

check_collator() {
    BIN=target/release/polkadot-parachain
    if [ -f $BIN ]; then
        echo "✅ Collator binary found:"
        $BIN --version
    else
        echo "❌ Collator binary not found, exiting"
        exit 1
    fi
}

set -e

chain_id=$1
rpc_endpoint=$2
work_dir="parachains/chain-specs"
chain_spec=$work_dir/$chain_id.json
chain_values=$work_dir/${chain_id}_values.json
chain_values_scale=$work_dir/${chain_id}_values.scale

[ -z "$chain_id" ] && usage
chain_spec_summary

if [ "$rpc_endpoint" == "" ]; then
    # default connecting to the official rpc
    rpc_endpoint='wss://statemint-shell.polkadot.io'
fi

if [[ "$rpc_endpoint" =~ "localhost" ]]; then
    check_collator
    echo -e "Make sure you have a collator running with the correct version at $rpc_endpoint."
    echo -e "If you don't, NOW is the time to start it with:"
    echo -e "target/release/polkadot-parachain --chain parachains/chain-specs/shell.json --tmp\n"
    read -p "You can abort with CTRL+C if this is not correct, otherwise press ENTER "
fi

echo "Generating genesis values..."
pushd scripts/generate_genesis_values
yarn
popd

node scripts/generate_genesis_values $chain_spec $chain_values

echo "Scale encoding..."
pushd scripts/scale_encode_genesis
yarn
popd

node scripts/scale_encode_genesis $chain_values $chain_values_scale $rpc_endpoint


ls -al parachains/chain-specs/${chain_id}_value*.*
