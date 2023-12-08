#!/usr/bin/env bash
set -eu

start=$(date +%s)

from_start_services=true

source scripts/set-env.sh
source scripts/build-binary.sh

trap kill_all SIGINT SIGTERM EXIT
cleanup

# 0. check required tools
echo "Check building tools"
check_tool

# 1. install binary if required
echo "Installing binaries if required"
install_binary

# 2. start ethereum
echo "Starting ethereum nodes"
if [ "$eth_network" == "localhost" ]; then
  source scripts/deploy-ethereum.sh
  deploy_ethereum
else
  source scripts/start-goerli.sh
  # deploy beacon node locally for fast response time or retrieving beacon state from remote could be very slow
  deploy_goerli
fi

# 3. start polkadot
echo "Starting polkadot nodes"
source scripts/deploy-polkadot.sh
deploy_polkadot

# 4. generate beefy checkpoint
echo "Generate beefy checkpoint"
source scripts/generate-beefy-checkpoint.sh
generate_beefy_checkpoint

# 5. deploy contracts
echo "Deploying ethereum contracts"
source scripts/deploy-contracts.sh
deploy_contracts

# 6. config bridgehub
echo "Config bridgehub"
source scripts/configure-bridgehub.sh
configure_bridgehub

if [ "$skip_relayer" == "false" ]; then
    # 7. start relayer
    echo "Starting relayers"
    source scripts/start-relayer.sh
    deploy_relayer
fi

echo "Testnet has been initialized"

end=$(date +%s)
runtime=$((end-start))
minutes=$(( (runtime % 3600) / 60 ));
seconds=$(( (runtime % 3600) % 60 ));
echo "Took $minutes minutes $seconds seconds"

wait
