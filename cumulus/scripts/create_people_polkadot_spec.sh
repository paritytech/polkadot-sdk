#!/usr/bin/env bash
set -euo pipefail

# This script generates the people-polkadot base chainspec at genesis. This is then modified by the
# script in polkadot-sdk on the nacho/people-chain-spec-with-migation branch, which also generates
# the genesis wasm and head data.

## Configure
para_id=1004
version="v1.2.6"
release_wasm="people-polkadot_runtime-v1002006.compact.compressed.wasm"
build_level="release"

## Download
if [ ! -f $release_wasm ]; then
  curl -OL "https://github.com/polkadot-fellows/runtimes/releases/download/$version/$release_wasm"
fi

if [ -d runtimes ]; then
  cd runtimes
  git fetch --tags
  cd -
else
  git clone https://github.com/polkadot-fellows/runtimes
fi

if [ -d polkadot-sdk ]; then
  cd polkadot-sdk
  git checkout master && git pull
  cd -
else
  git clone --branch master --single-branch https://github.com/paritytech/polkadot-sdk
fi

## Prepare
cd runtimes
git checkout tags/$version --force
cargo build -p chain-spec-generator --$build_level
cd -
chain_spec_generator="./runtimes/target/$build_level/chain-spec-generator"


cd polkadot-sdk
# After people-polkadot was supported, but before the breaking change which expects additional runtime api.
git checkout 68cdb12 --force
cargo build -p polkadot-parachain-bin --$build_level
cd -
polkadot_parachain="./polkadot-sdk/target/$build_level/polkadot-parachain"

# Dump the runtime to hex.
cat $release_wasm | od -A n -v -t x1 | tr -d ' \n' >rt-hex.txt

# Generate the local chainspec to manipulate.
$chain_spec_generator people-polkadot-local >chain-spec-plain.json

## Patch
# Related issue for Parity bootNodes, invulnerables, and session keys: https://github.com/paritytech/infrastructure/issues/6
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtimeGenesis.code = ("0x" + $code)' |
  jq '.name = "Polkadot People"' |
  jq '.id = "people-polkadot"' |
  jq '.chainType = "Live"' |
  jq '.bootNodes = [
    "/dns/polkadot-people-connect-0.polkadot.io/tcp/30334/p2p/12D3KooWP7BoJ7nAF9QnsreN8Eft1yHNUhvhxFiQyKFEUePi9mu3",
    "/dns/polkadot-people-connect-1.polkadot.io/tcp/30334/p2p/12D3KooWSSfWY3fTGJvGkuNUNBSNVCdLLNJnwkZSNQt7GCRYXu4o",
    "/dns/polkadot-people-connect-0.polkadot.io/tcp/443/wss/p2p/12D3KooWP7BoJ7nAF9QnsreN8Eft1yHNUhvhxFiQyKFEUePi9mu3",
    "/dns/polkadot-people-connect-1.polkadot.io/tcp/443/wss/p2p/12D3KooWSSfWY3fTGJvGkuNUNBSNVCdLLNJnwkZSNQt7GCRYXu4o"
  ]' |
  jq '.relay_chain = "polkadot"' |
  jq --argjson para_id $para_id '.para_id = $para_id' |
  jq --argjson para_id $para_id '.genesis.runtimeGenesis.patch.parachainInfo.parachainId = $para_id' |
  jq '.genesis.runtimeGenesis.patch.balances.balances = []' |
  jq '.genesis.runtimeGenesis.patch.collatorSelection.invulnerables = [
    "1CVdL7sb6AQGMQYZb8NfQhcBQMhmTLN3e7NDEby8rZkjyJo",
    "14QhqUX7kux5PggbBwUFFZNuLvfX2CjzUQ9V56m4d4S67Pgn",
    "112FKz5UNxjXqe3Wowe73a8FHnR5B4R9qi2pbMaXJczGNJsx",
    "16FyxKfMF3LnX4CmDsv1PUDPNwqDYiR7rKurwuJxSGgnTsH2",
    "14EQvBy9h8xGbh2R3ustnkfkF514E7wpmHtg27gDaTLM2str",
    "14sD2iYm1HsFPoHaT2GJNUMD2KJzvJNfVe9PBrG1KGyDBeHn",
    "1bLdd7zvNvjGpseQ8BGbGJekCppb1X5Gb228c9MQfHfmmBr"
  ]' |
  jq '.genesis.runtimeGenesis.patch.session.keys = [
    [
      "1CVdL7sb6AQGMQYZb8NfQhcBQMhmTLN3e7NDEby8rZkjyJo",
      "1CVdL7sb6AQGMQYZb8NfQhcBQMhmTLN3e7NDEby8rZkjyJo",
      {
        "aura": "1WyMcPD9qNrweNu6SKR1TTE2MybFiG8QsZSYxTMsFomuL1o"
      }
    ],
    [
      "14QhqUX7kux5PggbBwUFFZNuLvfX2CjzUQ9V56m4d4S67Pgn",
      "14QhqUX7kux5PggbBwUFFZNuLvfX2CjzUQ9V56m4d4S67Pgn",
      {
        "aura": "15wq6YmW6panxKmFaLEmrKpsypM2eT4VDY3JvrATnA6eMqvk"
      }
    ],
    [
      "112FKz5UNxjXqe3Wowe73a8FHnR5B4R9qi2pbMaXJczGNJsx",
      "112FKz5UNxjXqe3Wowe73a8FHnR5B4R9qi2pbMaXJczGNJsx",
      {
        "aura": "13Th3imMymWAXD54sMyTYAVyuWsz2GSix5SMAyHKszdFtSxc"
      }
    ],
    [
      "16FyxKfMF3LnX4CmDsv1PUDPNwqDYiR7rKurwuJxSGgnTsH2",
      "16FyxKfMF3LnX4CmDsv1PUDPNwqDYiR7rKurwuJxSGgnTsH2",
      {
        "aura": "14ii4R1kDMf4X1nLVHN2nGEu85ptTiFbAAaFzMGu2wcrCAJ5"
      }
    ],
    [
      "14EQvBy9h8xGbh2R3ustnkfkF514E7wpmHtg27gDaTLM2str",
      "14EQvBy9h8xGbh2R3ustnkfkF514E7wpmHtg27gDaTLM2str",
      {
        "aura": "12sBnnQpA3pV98pakjbc23cVSmpYdYxCEEs83FSybwWpS4Ub"
      }
    ],
    [
      "14sD2iYm1HsFPoHaT2GJNUMD2KJzvJNfVe9PBrG1KGyDBeHn",
      "14sD2iYm1HsFPoHaT2GJNUMD2KJzvJNfVe9PBrG1KGyDBeHn",
      {
        "aura": "12uVrDhFxe6Lx8U1eZtmfjsyohjB5TwLszij2pu4uiH4NGbF"
      }
    ],
    [
      "1bLdd7zvNvjGpseQ8BGbGJekCppb1X5Gb228c9MQfHfmmBr",
      "1bLdd7zvNvjGpseQ8BGbGJekCppb1X5Gb228c9MQfHfmmBr",
      {
        "aura": "14QNHMVxTUFs4HfPZoZtLpZXR9cvPhEDGvUyNPusznKVpCzC"
      }
    ]
  ]' |
  jq '.genesis.runtimeGenesis.patch.polkadotXcm.safeXcmVersion = 3' \
    > people-polkadot-genesis.json


## Convert to raw
$polkadot_parachain build-spec --raw --chain ./people-polkadot-genesis.json > people-polkadot.json

## Cleanup
rm -f rt-hex.txt
rm -f chain-spec-plain.json

echo "The genesis wasm and head data can now be generated using the script in polkadot-sdk on the nacho/people-chain-spec-with-migation branch. This will also modify the chainspec"
echo "See https://github.com/paritytech/polkadot-sdk/blob/0887f9ace688005ed78b21044b1b23bdab748c6d/cumulus/scripts/migrate_storage_to_genesis/README.md"