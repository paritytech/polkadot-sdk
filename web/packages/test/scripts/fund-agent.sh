#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

fund_agent() {
    pushd "$contract_dir"
    forge script \
        --rpc-url $eth_endpoint_http \
        --broadcast \
        -vvv \
        src/FundAgent.sol:FundAgent
    popd

    echo "Fund agent success!"
}

if [ -z "${from_start_services:-}" ]; then
    echo "Funding agent"
    fund_agent
fi
