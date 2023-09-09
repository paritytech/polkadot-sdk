#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./scripts/create_coretime_kusama_spec.sh ./target/release/wbuild/coretime-kusama-runtime/coretime_kusama_runtime.compact.compressed.wasm 1005"
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
$binary build-spec --chain coretime-kusama-dev > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
# TODO: Get bootNodes, invulnerables, and session keys https://github.com/paritytech/devops/issues/2725
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtime.system.code = ("0x" + $code)' \
    | jq '.name = "Kusama Coretime"' \
    | jq '.id = "coretime-kusama"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
            "/dns/kusama-coretime-connect-a-0.polkadot.io/tcp/30334/p2p/12D3KooWR7Biy6nPgQFhk2eYP62pAkcFA6he9RUFURTDh7ewTjpo",
            "/dns/kusama-coretime-connect-a-1.polkadot.io/tcp/30334/p2p/12D3KooWAGFiMZDF9RxdacrkenzGdo8nhfSe9EXofHc5mHeJ9vGX",
            "/dns/kusama-coretime-connect-b-0.polkadot.io/tcp/30334/p2p/12D3KooWEbJsTw3TnLjDr3M7LtuBzhSBeMThpgRRNF5zPP2PUnjM",
            "/dns/kusama-coretime-connect-b-1.polkadot.io/tcp/30334/p2p/12D3KooWMkSaSjV6pZ58d5zaBykQitYQaKtuD3TTWYbuES5WLdny",
            "/dns/kusama-coretime-connect-a-0.polkadot.io/tcp/443/wss/p2p/12D3KooWR7Biy6nPgQFhk2eYP62pAkcFA6he9RUFURTDh7ewTjpo",
            "/dns/kusama-coretime-connect-a-1.polkadot.io/tcp/443/wss/p2p/12D3KooWAGFiMZDF9RxdacrkenzGdo8nhfSe9EXofHc5mHeJ9vGX",
            "/dns/kusama-coretime-connect-b-0.polkadot.io/tcp/443/wss/p2p/12D3KooWEbJsTw3TnLjDr3M7LtuBzhSBeMThpgRRNF5zPP2PUnjM",
            "/dns/kusama-coretime-connect-b-1.polkadot.io/tcp/443/wss/p2p/12D3KooWMkSaSjV6pZ58d5zaBykQitYQaKtuD3TTWYbuES5WLdny"  
        ]' \
    | jq '.relay_chain = "kusama"' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtime.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtime.balances.balances = []' \
    | jq '.genesis.runtime.collatorSelection.invulnerables = [
            "HRn3a4qLmv1ejBHvEbnjaiEWjt154iFi2Wde7bXKGUwGvtL",
            "Cx9Uu2sxp3Xt1QBUbGQo7j3imTvjWJrqPF1PApDoy6UVkWP",
            "H9wzV7Uq383BHcywiTNQXHG36jFAGkThi6FZbe8HJXTXBCh",
            "HKuLwzkQivNK8uYnGsU2y2vYvbMahQuiyAjnjzZo6fDPKfU"
        ]' \
    | jq '.genesis.runtime.session.keys = [
            [
                "HRn3a4qLmv1ejBHvEbnjaiEWjt154iFi2Wde7bXKGUwGvtL",
                "HRn3a4qLmv1ejBHvEbnjaiEWjt154iFi2Wde7bXKGUwGvtL",
                    {
                        "aura": "0x4491cfc3ef17b4e02c66a7161f34fcacabf86ad64a783c1dbbe74e4ef82a7966"
                    }
            ],
            [
                "Cx9Uu2sxp3Xt1QBUbGQo7j3imTvjWJrqPF1PApDoy6UVkWP",
                "Cx9Uu2sxp3Xt1QBUbGQo7j3imTvjWJrqPF1PApDoy6UVkWP",
                    {
                        "aura": "0x04e3a3ecadbd493eb64ab2c19d215ccbc9eebea686dc3cea4833194674a8285e"
                    }
            ],
            [
                "H9wzV7Uq383BHcywiTNQXHG36jFAGkThi6FZbe8HJXTXBCh",
                "H9wzV7Uq383BHcywiTNQXHG36jFAGkThi6FZbe8HJXTXBCh",
                    {
                        "aura": "0xd6838cd2a39de890885e8a1c3c9c58a614f2bdd7b5740ee683a6bbc23703c244"
                    }
            ],
            [
                "HKuLwzkQivNK8uYnGsU2y2vYvbMahQuiyAjnjzZo6fDPKfU",
                "HKuLwzkQivNK8uYnGsU2y2vYvbMahQuiyAjnjzZo6fDPKfU",
                    {
                        "aura": "0x3cef8a7dc5acd430fc1471e56470ba55c950ee34789c3cf28bf951ce4b804364"
                    }
            ]
        ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json coretime-kusama-spec.json
cp chain-spec-raw.json ./parachains/chain-specs/coretime-kusama.json
cp chain-spec-raw.json coretime-kusama-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > coretime-kusama-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > coretime-kusama-wasm
