#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./cumulus/scripts/create_coretime_rococo_spec.sh ./target/release/wbuild/coretime-rococo-runtime/coretime_rococo_runtime.compact.compressed.wasm 1005"
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
$binary build-spec --chain coretime-rococo-dev > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
# Related issue for bootNodes, invulnerables, and session keys: https://github.com/paritytech/devops/issues/2725
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtimeGenesis.code = ("0x" + $code)' \
    | jq '.name = "Rococo Coretime"' \
    | jq '.id = "coretime-rococo"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
          "/dns/rococo-coretime-collator-node-0.polkadot.io/tcp/30333/p2p/12D3KooWHBUH9wGBx1Yq1ZePov9VL3AzxRPv5DTR4KadiCU6VKxy",
          "/dns/rococo-coretime-collator-node-1.polkadot.io/tcp/30333/p2p/12D3KooWB3SKxdj6kpwTkdMnHJi6YmadojCzmEqFkeFJjxN812XX"
        ]' \
    | jq '.relay_chain = "rococo"' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtimeGenesis.patch.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtimeGenesis.patch.balances.balances = []' \
    | jq '.genesis.runtimeGenesis.patch.collatorSelection.invulnerables = [
          "5G6Zua7Sowmt6ziddwUyueQs7HXDUVvDLaqqJDXXFyKvQ6Y6",
          "5C8aSedh7ShpWEPW8aTNEErbKkMbiibdwP8cRzVRNqLmzAWF"
        ]' \
    | jq '.genesis.runtimeGenesis.patch.session.keys = [
          [
            "5G6Zua7Sowmt6ziddwUyueQs7HXDUVvDLaqqJDXXFyKvQ6Y6",
            "5G6Zua7Sowmt6ziddwUyueQs7HXDUVvDLaqqJDXXFyKvQ6Y6",
            {
              "aura": "5G6Zua7Sowmt6ziddwUyueQs7HXDUVvDLaqqJDXXFyKvQ6Y6"
            }
          ],
          [
            "5C8aSedh7ShpWEPW8aTNEErbKkMbiibdwP8cRzVRNqLmzAWF",
            "5C8aSedh7ShpWEPW8aTNEErbKkMbiibdwP8cRzVRNqLmzAWF",
            {
              "aura": "5C8aSedh7ShpWEPW8aTNEErbKkMbiibdwP8cRzVRNqLmzAWF"
            }
          ]
        ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json coretime-rococo-spec.json
cp chain-spec-raw.json ./cumulus/parachains/chain-specs/coretime-rococo.json
cp chain-spec-raw.json coretime-rococo-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > coretime-rococo-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > coretime-rococo-wasm

# cleanup
rm -f rt-hex.txt
rm -f chain-spec-plain.json
rm -f chain-spec-raw.json
rm -f edited-chain-spec-plain.json
