#!/usr/bin/env bash
set -euo pipefail

# This script generates the people-kusama base chainspec at genesis. This is then modified by the
# script in polkadot-sdk on the nacho/people-chain-spec-with-migation branch, which also generates
# the genesis wasm and head data.
# The genesis spec for Kusama was generated with the script at commit 0887f9ace688005ed78b21044b1b23bdab748c6d.

## Configure
para_id=1004
version="v1.2.0"
release_wasm="people-kusama_runtime-v1002000.compact.compressed.wasm"
build_level="release" # Change to debug for faster builds

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
# After people-kusama was supported, but before the breaking change which expects additional runtime api.
git checkout 68cdb12 --force
cargo build -p polkadot-parachain-bin --$build_level
cd -
polkadot_parachain="./polkadot-sdk/target/$build_level/polkadot-parachain"

# Dump the runtime to hex.
cat $release_wasm | od -A n -v -t x1 | tr -d ' \n' >rt-hex.txt

# Generate the local chainspec to manipulate.
$chain_spec_generator people-kusama-local >chain-spec-plain.json

## Patch
# Related issue for Parity bootNodes, invulnerables, and session keys: https://github.com/paritytech/devops/issues/2725
cat chain-spec-plain.json | jq --rawfile code rt-hex.txt '.genesis.runtimeGenesis.code = ("0x" + $code)' |
  jq '.name = "Kusama People"' |
  jq '.id = "people-kusama"' |
  jq '.chainType = "Live"' |
  jq '.bootNodes = [
    "/dns/kusama-people-connect-0.polkadot.io/tcp/30334/p2p/12D3KooWQaqG5TNmDfRWrtH7tMsN7YeqwVkSfoZT4GkemSzezNi1",
    "/dns/kusama-people-connect-1.polkadot.io/tcp/30334/p2p/12D3KooWKhYoQH9LdSyvY3SVZY9gFf6ZV1bFh6317TRehUP3r5fm",
    "/dns/kusama-people-connect-0.polkadot.io/tcp/443/wss/p2p/12D3KooWQaqG5TNmDfRWrtH7tMsN7YeqwVkSfoZT4GkemSzezNi1",
    "/dns/kusama-people-connect-1.polkadot.io/tcp/443/wss/p2p/12D3KooWKhYoQH9LdSyvY3SVZY9gFf6ZV1bFh6317TRehUP3r5fm"
  ]' |
  jq '.relay_chain = "kusama"' |
  jq --argjson para_id $para_id '.para_id = $para_id' |
  jq --argjson para_id $para_id '.genesis.runtimeGenesis.patch.parachainInfo.parachainId = $para_id' |
  jq '.genesis.runtimeGenesis.patch.balances.balances = []' |
  jq '.genesis.runtimeGenesis.patch.collatorSelection.invulnerables = [
    "HNrgbuMxf7VLwsMd6YjnNQM6fc7VVsaoNVaMYTCCfK3TRWJ",
    "CuLgnS17KwfweeoN9y59YrhDG4pekfiY8qxieDaVTcVCjuP",
    "J11Rp4mjz3vRb2DL51HqRGRjhuEQRyXgtuFskebXb8zMZ9s",
    "H1tAQMm3eizGcmpAhL9aA9gR844kZpQfkU7pkmMiLx9jSzE",
    "CbLd7BdUr8DqD4TciR1kH6w12bbHBCW9n2MHGCtbxq4U5ty",
    "CdW8izFcLeicL3zZUQaC3a39AGeNSTgc9Jb5E5sjREPryA2",
    "HmatizNhXrZtXwQK2LfntvjCy3x1EuKs1WnRQ6CP3KkNfmA",
    "DtuntvQBh9vajFTnd42aTTCiuCyY3ep6EVwhhPji2ejyyhW",
    "HPUEzi4v3YJmhBfSbcGEFFiNKPAGVnGkfDiUzBNTR7j1CxT"
  ]' |
  jq '.genesis.runtimeGenesis.patch.session.keys = [
    [
      "HNrgbuMxf7VLwsMd6YjnNQM6fc7VVsaoNVaMYTCCfK3TRWJ",
      "HNrgbuMxf7VLwsMd6YjnNQM6fc7VVsaoNVaMYTCCfK3TRWJ",
      {
        "aura": "FF4CWRg8eGk8zEmGxswx4ppBQN96HdZhkV35YJU6rfXabpV"
      }
    ],
    [
      "CuLgnS17KwfweeoN9y59YrhDG4pekfiY8qxieDaVTcVCjuP",
      "CuLgnS17KwfweeoN9y59YrhDG4pekfiY8qxieDaVTcVCjuP",
      {
        "aura": "HEuPjdpQ3yv45zwk6h6985PNK8wszRyeAjDd4GJW5dZEpNp"
      }
    ],
    [
      "J11Rp4mjz3vRb2DL51HqRGRjhuEQRyXgtuFskebXb8zMZ9s",
      "J11Rp4mjz3vRb2DL51HqRGRjhuEQRyXgtuFskebXb8zMZ9s",
      {
        "aura": "H4s9sGNMvzdjFMKi8qMBqnxhGJR6T7Ytx6foFz9CVhGVyQn"
      }
    ],
    [
      "H1tAQMm3eizGcmpAhL9aA9gR844kZpQfkU7pkmMiLx9jSzE",
      "H1tAQMm3eizGcmpAhL9aA9gR844kZpQfkU7pkmMiLx9jSzE",
      {
        "aura": "Eis5y75gUQtH712YCyF5q6PjE8UsZzFJ4q3tSYQv2QifZKT"
      }
    ],
    [
      "CbLd7BdUr8DqD4TciR1kH6w12bbHBCW9n2MHGCtbxq4U5ty",
      "CbLd7BdUr8DqD4TciR1kH6w12bbHBCW9n2MHGCtbxq4U5ty",
      {
        "aura": "E7XKeXCdv3PF1UMmBMU8qH536LKvpwHcgFCVSUbYwK8QrqY"
      }
    ],
    [
      "CdW8izFcLeicL3zZUQaC3a39AGeNSTgc9Jb5E5sjREPryA2",
      "CdW8izFcLeicL3zZUQaC3a39AGeNSTgc9Jb5E5sjREPryA2",
      {
        "aura": "Cm8X6ekpTVidkFPUmDF7dHFLeWQyrdGW1RhEeuijeR2Pntd"
      }
    ],
    [
      "HmatizNhXrZtXwQK2LfntvjCy3x1EuKs1WnRQ6CP3KkNfmA",
      "HmatizNhXrZtXwQK2LfntvjCy3x1EuKs1WnRQ6CP3KkNfmA",
      {
        "aura": "GRvavY8h77mnRHbEQsFvUzWpw3kvH8164aVUgKqoyMW8rpV"
      }
    ],
    [
      "DtuntvQBh9vajFTnd42aTTCiuCyY3ep6EVwhhPji2ejyyhW",
      "DtuntvQBh9vajFTnd42aTTCiuCyY3ep6EVwhhPji2ejyyhW",
      {
        "aura": "HeSr4JUpXgrfKNwZGcJYU5FSn3znDoZaXnYxWB168bw5WUM"
      }
    ],
    [
      "HPUEzi4v3YJmhBfSbcGEFFiNKPAGVnGkfDiUzBNTR7j1CxT",
      "HPUEzi4v3YJmhBfSbcGEFFiNKPAGVnGkfDiUzBNTR7j1CxT",
      {
        "aura": "HppWoUUWibaZn3zgmcaWZY3BLbZzRktLiNK5e6DUBxHuniE"
      }
    ]
  ]' |
  jq '.genesis.runtimeGenesis.patch.polkadotXcm.safeXcmVersion = 3' \
    > people-kusama-genesis.json


## Convert to raw
$polkadot_parachain build-spec --raw --chain ./people-kusama-genesis.json > people-kusama.json

## Cleanup
rm -f rt-hex.txt
rm -f chain-spec-plain.json

echo "The genesis wasm and head data can now be generated using the script in polkadot-sdk on the nacho/people-chain-spec-with-migation branch. This will also modify the chainspec"
echo "See https://github.com/paritytech/polkadot-sdk/blob/0887f9ace688005ed78b21044b1b23bdab748c6d/cumulus/scripts/migrate_storage_to_genesis/README.md"
