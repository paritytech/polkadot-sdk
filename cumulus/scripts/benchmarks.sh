#!/usr/bin/env bash

target=${1:-production}
steps=${2:-50}
repeat=${3:-20}

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

${__dir}/benchmarks-ci.sh collectives collectives-polkadot target/$target $steps $repeat

${__dir}/benchmarks-ci.sh assets asset-hub-kusama target/$target $steps $repeat
${__dir}/benchmarks-ci.sh assets asset-hub-polkadot target/$target $steps $repeat
${__dir}/benchmarks-ci.sh assets asset-hub-westend target/$target $steps $repeat

${__dir}/benchmarks-ci.sh bridge-hubs bridge-hub-polkadot target/$target $steps $repeat
${__dir}/benchmarks-ci.sh bridge-hubs bridge-hub-kusama target/$target $steps $repeat
${__dir}/benchmarks-ci.sh bridge-hubs bridge-hub-rococo target/$target $steps $repeat

${__dir}/benchmarks-ci.sh glutton glutton-kusama target/$target $steps $repeat
