#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./scripts/create_bridge_hub_westend_spec.sh ./target/release/wbuild/bridge-hub-kusama-runtime/bridge_hub_kusama_runtime.compact.compressed.wasm 1002"
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
$binary build-spec --chain bridge-hub-kusama-dev > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtime.system.code = ("0x" + $code)' \
    | jq '.name = "Westend BridgeHub"' \
    | jq '.id = "bridge-hub-westend"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
                               "/dns/westend-bridge-hub-collator-node-0.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWKyEuqkkWvFSrwZWKWBAsHgLV3HGfHj7yH3LNJLAVhmxY",
                               "/dns/westend-bridge-hub-collator-node-1.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWBpvudthz61XC4oP2YYFFJdhWohBeQ1ffn1BMSGWhapjd",
                               "/dns/westend-bridge-hub-collator-node-2.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWPXqdRRthjKAMPFtaXUK7yBxsvh83QsmzXzALA3inoJfo",
                               "/dns/westend-bridge-hub-collator-node-3.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWAp2YpVaiNBy7rozEHJGocDpaLFt3VFZsGMBEYh4BoEz7"
                       ]' \
    | jq '.relay_chain = "westend"' \
    | jq '.properties = {
                                    "tokenDecimals": 12,
                                    "tokenSymbol": "WND"
                        }' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtime.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtime.balances.balances = []' \
    | jq '.genesis.runtime.collatorSelection.invulnerables = [
                                                                "5GN5qBbUkxigdLhTajWqAG66MRD2v5WjUFqkuGVSRCyhMCg6",
                                                                "5GRCPWstCyp3u9T2c3oGqj83rniQffJR5Q2LpGsL9m19oQ8T",
                                                                "5GR2p9FpJFPpDuZPk1Lt9VZJ76aLPfKVA6qBE4FRted2oT6D",
                                                                "5FH8VBgdXijT1vM6pj1aFGw49J2fQDZKM1BFQtVV1zjmA7mM"
                                                             ]' \
    | jq '.genesis.runtime.session.keys = [
                                            [
                                                "5GN5qBbUkxigdLhTajWqAG66MRD2v5WjUFqkuGVSRCyhMCg6",
                                                "5GN5qBbUkxigdLhTajWqAG66MRD2v5WjUFqkuGVSRCyhMCg6",
                                                    {
                                                        "aura": "5GN5qBbUkxigdLhTajWqAG66MRD2v5WjUFqkuGVSRCyhMCg6"
                                                    }
                                            ],
                                            [
                                                "5GRCPWstCyp3u9T2c3oGqj83rniQffJR5Q2LpGsL9m19oQ8T",
                                                "5GRCPWstCyp3u9T2c3oGqj83rniQffJR5Q2LpGsL9m19oQ8T",
                                                    {
                                                        "aura": "5GRCPWstCyp3u9T2c3oGqj83rniQffJR5Q2LpGsL9m19oQ8T"
                                                    }
                                            ],
                                            [
                                                "5GR2p9FpJFPpDuZPk1Lt9VZJ76aLPfKVA6qBE4FRted2oT6D",
                                                "5GR2p9FpJFPpDuZPk1Lt9VZJ76aLPfKVA6qBE4FRted2oT6D",
                                                    {
                                                        "aura": "5GR2p9FpJFPpDuZPk1Lt9VZJ76aLPfKVA6qBE4FRted2oT6D"
                                                    }
                                            ],
                                            [
                                                "5FH8VBgdXijT1vM6pj1aFGw49J2fQDZKM1BFQtVV1zjmA7mM",
                                                "5FH8VBgdXijT1vM6pj1aFGw49J2fQDZKM1BFQtVV1zjmA7mM",
                                                    {
                                                        "aura": "5FH8VBgdXijT1vM6pj1aFGw49J2fQDZKM1BFQtVV1zjmA7mM"
                                                    }
                                            ]
                                          ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json bridge-hub-westend-spec.json
cp chain-spec-raw.json ./parachains/chain-specs/bridge-hub-westend.json
cp chain-spec-raw.json bridge-hub-westend-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > bridge-hub-westend-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > bridge-hub-westend-wasm
