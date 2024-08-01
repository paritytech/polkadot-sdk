#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./cumulus/scripts/create_people_westend_spec.sh ./target/release/wbuild/people-westend-runtime/people_westend_runtime.compact.compressed.wasm 1004"
    exit 1
}

if [ -z "$1" ]; then
    usage
fi

if [ -z "$2" ]; then
    usage
fi

set -e

rt_path=$1
para_id=$2

echo "Generating chain spec for runtime: $rt_path and para_id: $para_id"

binary="./target/release/polkadot-parachain"

# build the chain spec we'll manipulate
$binary build-spec --chain people-westend-local > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
# Boot nodes, invulnerables, and session keys from https://github.com/paritytech/devops/issues/2847
#
# Note: This is a testnet runtime. Each invulnerable's Aura key is also used as its AccountId. This
# is not recommended in value-bearing networks.
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtimeGenesis.code = ("0x" + $code)' \
    | jq '.name = "Westend People"' \
    | jq '.id = "people-westend"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
        "/dns/westend-people-collator-node-0.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWDcLjDLTu9fNhmas9DTWtqdv8eUbFMWQzVwvXRK7QcjHD",
        "/dns/westend-people-collator-node-0.parity-testnet.parity.io/tcp/443/wss/p2p/12D3KooWDcLjDLTu9fNhmas9DTWtqdv8eUbFMWQzVwvXRK7QcjHD",
        "/dns/westend-people-collator-node-1.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWM56JbKWAXsDyWh313z73aKYVMp1Hj2nSnAKY3q6MnoC9",
        "/dns/westend-people-collator-node-1.parity-testnet.parity.io/tcp/443/wss/p2p/12D3KooWM56JbKWAXsDyWh313z73aKYVMp1Hj2nSnAKY3q6MnoC9",
        "/dns/westend-people-collator-node-2.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWGVYTVKW7tYe51JvetvGvVLDPXzqQX1mueJgz14FgkmHG",
        "/dns/westend-people-collator-node-2.parity-testnet.parity.io/tcp/443/wss/p2p/12D3KooWGVYTVKW7tYe51JvetvGvVLDPXzqQX1mueJgz14FgkmHG",
        "/dns/westend-people-collator-node-3.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWCF1eA2Gap69zgXD7Df3e9DqDUsGoByocggTGejoHjK23",
        "/dns/westend-people-collator-node-3.parity-testnet.parity.io/tcp/443/wss/p2p/12D3KooWCF1eA2Gap69zgXD7Df3e9DqDUsGoByocggTGejoHjK23"
    ]' \
    | jq '.relay_chain = "westend"' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtimeGenesis.patch.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtimeGenesis.patch.balances.balances = []' \
    | jq '.genesis.runtimeGenesis.patch.collatorSelection.invulnerables = [
        "5CFYvshLff1dHmT33jUcBc7mEKbVRJKbA9HzPqmLfjksHah6",
        "5HgEdsYyVGVsyNmbE1sUxeDLrxTLJXnAKCNa2HJ9QXXEir1B",
        "5EZmD6eA9wm1Y2Dy2wefLCsFJJcC7o8bVfWm7Mfbuanc8JYo",
        "5EkJFfUtbo258dCaqgYSvajN1tNtXhT3SrybW8ZhygoMP3kE"
    ]' \
    | jq '.genesis.runtimeGenesis.patch.session.keys = [
            [
                "5CFYvshLff1dHmT33jUcBc7mEKbVRJKbA9HzPqmLfjksHah6",
                "5CFYvshLff1dHmT33jUcBc7mEKbVRJKbA9HzPqmLfjksHah6",
                    {
                        "aura": "5CFYvshLff1dHmT33jUcBc7mEKbVRJKbA9HzPqmLfjksHah6"
                    }
            ],
            [
                "5HgEdsYyVGVsyNmbE1sUxeDLrxTLJXnAKCNa2HJ9QXXEir1B",
                "5HgEdsYyVGVsyNmbE1sUxeDLrxTLJXnAKCNa2HJ9QXXEir1B",
                    {
                        "aura": "5HgEdsYyVGVsyNmbE1sUxeDLrxTLJXnAKCNa2HJ9QXXEir1B"
                    }
            ],
            [
                "5EZmD6eA9wm1Y2Dy2wefLCsFJJcC7o8bVfWm7Mfbuanc8JYo",
                "5EZmD6eA9wm1Y2Dy2wefLCsFJJcC7o8bVfWm7Mfbuanc8JYo",
                    {
                        "aura": "5EZmD6eA9wm1Y2Dy2wefLCsFJJcC7o8bVfWm7Mfbuanc8JYo"
                    }
            ],
            [
                "5EkJFfUtbo258dCaqgYSvajN1tNtXhT3SrybW8ZhygoMP3kE",
                "5EkJFfUtbo258dCaqgYSvajN1tNtXhT3SrybW8ZhygoMP3kE",
                    {
                        "aura": "5EkJFfUtbo258dCaqgYSvajN1tNtXhT3SrybW8ZhygoMP3kE"
                    }
            ]
        ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json people-westend-spec.json
cp chain-spec-raw.json ./cumulus/parachains/chain-specs/people-westend.json
cp chain-spec-raw.json people-westend-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > people-westend-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > people-westend-wasm
