#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./scripts/create_bridge_hub_polkadot_spec.sh ./target/release/wbuild/bridge-hub-polkadot-runtime/bridge_hub_polkadot_runtime.compact.compressed.wasm 1002"
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
$binary build-spec --chain bridge-hub-polkadot-dev > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtime.system.code = ("0x" + $code)' \
    | jq '.name = "Polkadot BridgeHub"' \
    | jq '.id = "bridge-hub-polkadot"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
                        "/dns/polkadot-bridge-hub-connect-a-0.polkadot.io/tcp/30334/p2p/12D3KooWAVQMhkXmc5ueSYasdsRWQbKus2YGZ6HDZUB4ViJMCxXy",
                        "/dns/polkadot-bridge-hub-connect-a-1.polkadot.io/tcp/30334/p2p/12D3KooWG4ypDHLKGCv4BZ6PuaGUwQHKAH6p2D6arR2uQ1eiR1T3",
                        "/dns/polkadot-bridge-hub-connect-b-0.polkadot.io/tcp/30334/p2p/12D3KooWCwGKxjpJXnx1mwXKvaxGQm769EM3b6Pg5vbU33wbhsNw",
                        "/dns/polkadot-bridge-hub-connect-b-1.polkadot.io/tcp/30334/p2p/12D3KooWLiSEdhriJUPdZKFtAjZrQncxN2ssEoDKVrt5mGM4Qu4J",

                        "/dns/polkadot-bridge-hub-connect-a-0.polkadot.io/tcp/443/wss/p2p/12D3KooWAVQMhkXmc5ueSYasdsRWQbKus2YGZ6HDZUB4ViJMCxXy",
                        "/dns/polkadot-bridge-hub-connect-a-1.polkadot.io/tcp/443/wss/p2p/12D3KooWG4ypDHLKGCv4BZ6PuaGUwQHKAH6p2D6arR2uQ1eiR1T3",
                        "/dns/polkadot-bridge-hub-connect-b-0.polkadot.io/tcp/443/wss/p2p/12D3KooWCwGKxjpJXnx1mwXKvaxGQm769EM3b6Pg5vbU33wbhsNw",
                        "/dns/polkadot-bridge-hub-connect-b-1.polkadot.io/tcp/443/wss/p2p/12D3KooWLiSEdhriJUPdZKFtAjZrQncxN2ssEoDKVrt5mGM4Qu4J"
                       ]' \
    | jq '.relay_chain = "polkadot"' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtime.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtime.balances.balances = []' \
    | jq '.genesis.runtime.collatorSelection.invulnerables = [
                                                                "134AK3RiMA97Fx9dLj1CvuLJUa8Yo93EeLA1TkP6CCGnWMSd",
                                                                "15dU8Tt7kde2diuHzijGbKGPU5K8BPzrFJfYFozvrS1DdE21",
                                                                "1vXMKM8SctM28AQw1wSpd7p9yCUWn1uhbbKSVTuznsw8Q2x",
                                                                "15mCQcaj3QP1UdxBF82JRd9v3riZJcVNVEmx8xkFp7DSYR4Y"
                                                             ]' \
    | jq '.genesis.runtime.session.keys = [
                                              [
                                                "134AK3RiMA97Fx9dLj1CvuLJUa8Yo93EeLA1TkP6CCGnWMSd",
                                                "134AK3RiMA97Fx9dLj1CvuLJUa8Yo93EeLA1TkP6CCGnWMSd",
                                                {
                                                  "aura": "5EX6AnyuSPEFQ7HAPjRgzqk1sxgh8cyacGimwJ16y1nJ2w7g"
                                                }
                                              ],
                                              [
                                                "15dU8Tt7kde2diuHzijGbKGPU5K8BPzrFJfYFozvrS1DdE21",
                                                "15dU8Tt7kde2diuHzijGbKGPU5K8BPzrFJfYFozvrS1DdE21",
                                                {
                                                  "aura": "5DZN8UhaJftvKhMMARmJBwrwzuEDpoUzzBvvWMbFXYsJ4CmK"
                                                }
                                              ],
                                              [
                                                "1vXMKM8SctM28AQw1wSpd7p9yCUWn1uhbbKSVTuznsw8Q2x",
                                                "1vXMKM8SctM28AQw1wSpd7p9yCUWn1uhbbKSVTuznsw8Q2x",
                                                {
                                                  "aura": "5FKsn83rXQQiw7HwoeYoLMoYS5GP9YVNHZiCHwA4DSwDcPVa"
                                                }
                                              ],
                                              [
                                                "15mCQcaj3QP1UdxBF82JRd9v3riZJcVNVEmx8xkFp7DSYR4Y",
                                                "15mCQcaj3QP1UdxBF82JRd9v3riZJcVNVEmx8xkFp7DSYR4Y",
                                                {
                                                  "aura": "5DCg19ckcJz4m52Th4o1LcSRK3H7NsUcQsRbu7pTDM3mZ26v"
                                                }
                                              ]
                                          ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json bridge-hub-polkadot-spec.json
cp chain-spec-raw.json ./parachains/chain-specs/bridge-hub-polkadot.json
cp chain-spec-raw.json bridge-hub-polkadot-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > bridge-hub-polkadot-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > bridge-hub-polkadot-wasm

# cleanup
rm -f rt-hex.txt
rm -f chain-spec-plain.json
rm -f chain-spec-raw.json
rm -f edited-chain-spec-plain.json
