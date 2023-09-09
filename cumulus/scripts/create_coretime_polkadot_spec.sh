#!/usr/bin/env bash

usage() {
    echo Usage:
    echo "$1 <srtool compressed runtime path>"
    echo "$2 <para_id>"
    echo "e.g.: ./scripts/create_coretime_polkadot_spec.sh ./target/release/wbuild/coretime-polkadot-runtime/coretime_polkadot_runtime.compact.compressed.wasm 1005"
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
$binary build-spec --chain coretime-polkadot-dev > chain-spec-plain.json

# convert runtime to hex
cat $rt_path | od -A n -v -t x1 |  tr -d ' \n' > rt-hex.txt

# replace the runtime in the spec with the given runtime and set some values to production
# TODO: Get bootNodes, invulnerables, and session keys https://github.com/paritytech/devops/issues/2725
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtime.system.code = ("0x" + $code)' \
    | jq '.name = "Polkadot Coretime"' \
    | jq '.id = "coretime-polkadot"' \
    | jq '.chainType = "Live"' \
    | jq '.bootNodes = [
            "/dns/polkadot-coretime-connect-a-0.polkadot.io/tcp/30334/p2p/12D3KooWKjnixAHbKMsPTJwGx8SrBeGEJLHA8KmKcEDYMp3YmWgR",
            "/dns/polkadot-coretime-connect-a-1.polkadot.io/tcp/30334/p2p/12D3KooWQ7B7p4DFv1jWqaKfhrZBcMmi5g8bWFnmskguLaGEmT6n",
            "/dns/polkadot-coretime-connect-b-0.polkadot.io/tcp/30334/p2p/12D3KooWSY6xCqhviY6wEDr6i9d53DMKTWcHex4FcZr8avHfGiHA",
            "/dns/polkadot-coretime-connect-b-1.polkadot.io/tcp/30334/p2p/12D3KooWQec5fBZjEtzcAYJk2u53ZnpHcceCG9B27WaUNgXoKW4F",
            "/dns/polkadot-coretime-connect-a-0.polkadot.io/tcp/443/wss/p2p/12D3KooWKjnixAHbKMsPTJwGx8SrBeGEJLHA8KmKcEDYMp3YmWgR",
            "/dns/polkadot-coretime-connect-a-1.polkadot.io/tcp/443/wss/p2p/12D3KooWQ7B7p4DFv1jWqaKfhrZBcMmi5g8bWFnmskguLaGEmT6n",
            "/dns/polkadot-coretime-connect-b-0.polkadot.io/tcp/443/wss/p2p/12D3KooWSY6xCqhviY6wEDr6i9d53DMKTWcHex4FcZr8avHfGiHA",
            "/dns/polkadot-coretime-connect-b-1.polkadot.io/tcp/443/wss/p2p/12D3KooWQec5fBZjEtzcAYJk2u53ZnpHcceCG9B27WaUNgXoKW4F"
        ]' \
    | jq '.relay_chain = "polkadot"' \
    | jq --argjson para_id $para_id '.para_id = $para_id' \
    | jq --argjson para_id $para_id '.genesis.runtime.parachainInfo.parachainId = $para_id' \
    | jq '.genesis.runtime.balances.balances = []' \
    | jq '.genesis.runtime.collatorSelection.invulnerables = [
            "13umUoWwGb765EPzMUrMmYTcEjKfNJiNyCDwdqAvCMzteGzi",
            "13NAwtroa2efxgtih1oscJqjxcKpWJeQF8waWPTArBewi2CQ",
            "162qThZRtVLKainHKQXGeS3iEjCkGg1XmySXZiexFXf9YPbv",
            "13aoMEErH2F17vL58mJ9gtNya6xYvVRTF6cFzUf7PGAu4BVN"
        ]' \
    | jq '.genesis.runtime.session.keys = [
            [
                "13umUoWwGb765EPzMUrMmYTcEjKfNJiNyCDwdqAvCMzteGzi",
                "13umUoWwGb765EPzMUrMmYTcEjKfNJiNyCDwdqAvCMzteGzi",
                    {
                        "aura": "0x4a69b6ec0eda668471d806db625681a147efc35a4baeacf0bca95d12d13cd942"
                    }
            ],
            [
                "13NAwtroa2efxgtih1oscJqjxcKpWJeQF8waWPTArBewi2CQ",
                "13NAwtroa2efxgtih1oscJqjxcKpWJeQF8waWPTArBewi2CQ",
                    {
                        "aura": "0xf0d0e90c36f95605510f00a9f0821675bc0c7b70e5c8d113b0426c21d627773b"
                    }
            ],
            [
                "162qThZRtVLKainHKQXGeS3iEjCkGg1XmySXZiexFXf9YPbv",
                "162qThZRtVLKainHKQXGeS3iEjCkGg1XmySXZiexFXf9YPbv",
                    {
                        "aura": "0x7eef7ea441b57ec8733ee9421b4362ecc18d4363e36f6cd7b4f87577aa15fc56"
                    }
            ],
            [
                "13aoMEErH2F17vL58mJ9gtNya6xYvVRTF6cFzUf7PGAu4BVN",
                "13aoMEErH2F17vL58mJ9gtNya6xYvVRTF6cFzUf7PGAu4BVN",
                    {
                        "aura": "0x78053fb2e32e35bbf13890a34dbdd00fd610843740235f5c397a76d19a27aa45"
                    }
            ]
        ]' \
    > edited-chain-spec-plain.json

# build a raw spec
$binary build-spec --chain edited-chain-spec-plain.json --raw > chain-spec-raw.json
cp edited-chain-spec-plain.json coretime-polkadot-spec.json
cp chain-spec-raw.json ./parachains/chain-specs/coretime-polkadot.json
cp chain-spec-raw.json coretime-polkadot-spec-raw.json

# build genesis data
$binary export-genesis-state --chain chain-spec-raw.json > coretime-polkadot-genesis-head-data

# build genesis wasm
$binary export-genesis-wasm --chain chain-spec-raw.json > coretime-polkadot-wasm
