#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$0 <srtool compressed runtime>"
    exit 1
}

set -e

rt_path=$1

binary="./target/release/polkadot-parachain"

# build the chain spec we'll manipulate
$binary build-spec --chain shell > shell-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > shell-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
cat shell-spec-plain.json | jq --rawfile code shell-hex.txt '.genesis.runtime.system.code = ("0x" + $code)' \
    | jq '.name = "Shell"' \
    | jq '.id = "shell"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = ["/ip4/34.65.116.156/tcp/30334/p2p/12D3KooWMdwvej593sntpXcxpUaFcsjc1EpCr5CL1JMoKmEhgj1N", "/ip4/34.65.105.127/tcp/30334/p2p/12D3KooWRywSWa2sQpcRuLhSeNSEs6bepLGgcdxFg8P7jtXRuiYf", "/ip4/34.65.142.204/tcp/30334/p2p/12D3KooWDGnPd5PzgvcbSwXsCBN3kb1dWbu58sy6R7h4fJGnZtq5", "/ip4/34.65.32.100/tcp/30334/p2p/12D3KooWSzHX7A3t6BwUQrq8R9ZVWLrfyYgkYLfpKMcRs14oFSgc"]' \
    | jq '.relay_chain = "polkadot"' \
    > edited-shell-plain.json

# build a raw spec
$binary build-spec --chain edited-shell-plain.json --raw > shell-spec-raw.json

# build genesis data
$binary export-genesis-state --parachain-id=1000 --chain shell-spec-raw.json > shell-head-data

# build genesis wasm
$binary export-genesis-wasm --chain shell-spec-raw.json > shell-wasm
