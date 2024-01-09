#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

deploy_contracts()
{
    pushd "$contract_dir"
    if [ "$eth_network" != "localhost" ]; then
        forge script \
            --rpc-url $eth_endpoint_http \
            --broadcast \
            --verify \
            --etherscan-api-key $etherscan_api_key \
            -vvv \
            src/DeployScript.sol:DeployScript
    else
        forge script \
            --rpc-url $eth_endpoint_http \
            --broadcast \
            -vvv \
            src/DeployScript.sol:DeployScript
    fi
    popd

    pushd "$test_helpers_dir"
    pnpm generateContracts "$output_dir/contracts.json"
    popd

    echo "Exported contract artifacts: $output_dir/contracts.json"
}

if [ -z "${from_start_services:-}" ]; then
    echo "Deploying contracts"
    deploy_contracts
fi
