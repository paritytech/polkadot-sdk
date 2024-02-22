#!/bin/bash

bridged_chain=$1
shift

# Add Alice as bridge owner
# We do this only if there is a `.genesis.runtimeGenesis.patch` object.
# Otherwise we're working with the raw chain spec.
$CHAIN_SPEC_GEN_BINARY "$@" \
  | jq 'if .genesis.runtimeGenesis.patch
    then .genesis.runtimeGenesis.patch.bridge'$bridged_chain'Grandpa.owner = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
    else .
    end'