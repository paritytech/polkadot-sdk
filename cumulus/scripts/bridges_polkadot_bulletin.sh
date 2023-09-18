#!/bin/bash

# import common functions
source "$(dirname "$0")"/bridges_rococo_wococo.sh "import"

function init_ksm_dot() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay init-bridge polkadot-bulletin-to-bridge-hub-polkadot \
	--source-host localhost \
	--source-port 9942 \
	--source-version-mode Auto \
	--target-host localhost \
	--target-port 8945 \
	--target-version-mode Auto \
	--target-signer //Bob
}

function init_dot_ksm() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay init-bridge polkadot-to-polkadot-bulletin \
        --source-host localhost \
        --source-port 9945 \
        --source-version-mode Auto \
        --target-host localhost \
        --target-port 9942 \
        --target-version-mode Auto \
        --target-signer //Alice
}

function run_relay() {
    echo OK
    ensure_relayer

    RUST_LOG=rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay relay-headers-and-messages polkadot-bulletin-bridge-hub-polkadot \
        --polkadot-bulletin-host localhost \
        --polkadot-bulletin-port 9942 \
        --polkadot-bulletin-version-mode Auto \
        --polkadot-bulletin-signer //Alice \
        --polkadot-bulletin-transactions-mortality 4 \
        --polkadot-host localhost \
        --polkadot-port 9945 \
        --bridge-hub-polkadot-host localhost \
        --bridge-hub-polkadot-port 8945 \
        --bridge-hub-polkadot-version-mode Auto \
        --bridge-hub-polkadot-signer //Charlie \
        --bridge-hub-polkadot-transactions-mortality 4 \
        --lane 00000000
}

case "$1" in
  run-relay)
    init_ksm_dot
    init_dot_ksm
    run_relay
    ;;
  stop)
    pkill -f polkadot
    pkill -f parachain
    ;;
  *)
    echo "A command is require. Supported commands for:
    Local (zombienet) run:
          - run-relay";
    exit 1
    ;;
esac
