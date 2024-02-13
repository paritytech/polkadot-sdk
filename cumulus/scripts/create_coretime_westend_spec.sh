#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./cumulus/scripts/create_coretime_westend_spec.sh ./target/release/wbuild/coretime-westend-runtime/coretime_westend_runtime.compact.compressed.wasm 1005"
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
$binary build-spec --chain coretime-westend-dev > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
# Related issue for bootNodes, invulnerables, and session keys: https://github.com/paritytech/devops/issues/2725
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtimeGenesis.code = ("0x" + $code)' \
    | jq '.name = "Westend Coretime"' \
    | jq '.id = "coretime-westend"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
          "/dns/westend-coretime-collator-0.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWP93Dzk8T7GWxyWw9jhLcz8Pksokk3R9vL2eEH337bNkT",
          "/dns/westend-coretime-collator-1.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWMh2imeAzsZKGQgm2cv6Uoep3GBYtwGfujt1bs5YfVzkH",
          "/dns/westend-coretime-collator-2.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWAys2hVpF7AN8hYGnu1T6XYFRGKeBFqD8q5LUcvWXRLg8",
          "/dns/westend-coretime-collator-3.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWSGgmiRryoi7A3qAmeYWgmVeGQkk66PrhDjJ6ZPP555as",
          "/dns/westend-coretime-connect-0.polkadot.io/tcp/443/wss/p2p/12D3KooWP93Dzk8T7GWxyWw9jhLcz8Pksokk3R9vL2eEH337bNkT",
          "/dns/westend-coretime-connect-1.polkadot.io/tcp/443/wss/p2p/12D3KooWMh2imeAzsZKGQgm2cv6Uoep3GBYtwGfujt1bs5YfVzkH",
          "/dns/westend-coretime-connect-2.polkadot.io/tcp/443/wss/p2p/12D3KooWAys2hVpF7AN8hYGnu1T6XYFRGKeBFqD8q5LUcvWXRLg8",
          "/dns/westend-coretime-connect-3.polkadot.io/tcp/443/wss/p2p/12D3KooWSGgmiRryoi7A3qAmeYWgmVeGQkk66PrhDjJ6ZPP555as",
        ]' \
    | jq '.relay_chain = "westend"' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtimeGenesis.patch.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtimeGenesis.patch.balances.balances = []' \
    | jq '.genesis.runtimeGenesis.patch.collatorSelection.invulnerables = [
          "5GKXTtB7RG3mLJ2kT4AkDXoxvKCFDVUdwyRmeMEbX3gBwcGi",
          "5DknBCD1h49nc8eqnm6XtHz3bMQm5hfMuGYcLenRfCmpnBJG",
          "5D52g9Mt9jQnZn6hwYhv649QYqGwhjygxkpb6rm3FYzYHEs3",
          "5Egx2B41PYj8uvuhkNJeucA54h6Xmi7ZH9wqrZLwj3CuvQKA"
        ]' \
    | jq '.genesis.runtimeGenesis.patch.session.keys = [
          [
            "5GKXTtB7RG3mLJ2kT4AkDXoxvKCFDVUdwyRmeMEbX3gBwcGi",
            "5GKXTtB7RG3mLJ2kT4AkDXoxvKCFDVUdwyRmeMEbX3gBwcGi",
            {
              "aura": "0xbc3ea120d2991b75447b0b53cd8623970a0f6d98fa2701036c74d94e6b79252c"
            }
          ],
          [
            "5DknBCD1h49nc8eqnm6XtHz3bMQm5hfMuGYcLenRfCmpnBJG",
            "5DknBCD1h49nc8eqnm6XtHz3bMQm5hfMuGYcLenRfCmpnBJG",
            {
              "aura": "0x4acc970c28713ec93bf925352d3023418fdf89933227e1e2fdae8481103dfe28"
            }
          ],
          [
            "5D52g9Mt9jQnZn6hwYhv649QYqGwhjygxkpb6rm3FYzYHEs3",
            "5D52g9Mt9jQnZn6hwYhv649QYqGwhjygxkpb6rm3FYzYHEs3",
            {
              "aura": "0x2c7b95155708c10616b6f1a77a84f3d92c9a0272609ed24dbb7e6bdb81b53e76"
            }
          ],
          [
            "5Egx2B41PYj8uvuhkNJeucA54h6Xmi7ZH9wqrZLwj3CuvQKA",
            "5Egx2B41PYj8uvuhkNJeucA54h6Xmi7ZH9wqrZLwj3CuvQKA",
            {
              "aura": "0x741cfb39ec61bc76824ccec62d61670a80a890e0e21d58817f84040d3ec54474"
            }
          ]
        ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json coretime-westend-spec.json
cp chain-spec-raw.json ./cumulus/parachains/chain-specs/coretime-westend.json
cp chain-spec-raw.json coretime-westend-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > coretime-westend-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > coretime-westend-wasm

# cleanup
rm -f rt-hex.txt
rm -f chain-spec-plain.json
rm -f chain-spec-raw.json
rm -f edited-chain-spec-plain.json
