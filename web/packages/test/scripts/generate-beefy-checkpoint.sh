#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

generate_beefy_checkpoint()
{
    pushd "$test_helpers_dir"
    pnpm generateBeefyCheckpoint
    popd
}

if [ -z "${from_start_services:-}" ]; then
    echo "generate beefy checkpoint!"
    generate_beefy_checkpoint
    wait
fi
