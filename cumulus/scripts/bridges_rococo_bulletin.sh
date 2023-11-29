#!/bin/bash

# import common functions
source "$(dirname "$0")"/bridges_rococo_westend.sh "import"

function init_bulletin_rococo() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay init-bridge rococo-bulletin-to-bridge-hub-rococo \
	--source-host localhost \
	--source-port 10000 \
	--source-version-mode Auto \
	--target-host localhost \
	--target-port 8943 \
	--target-version-mode Auto \
	--target-signer //Bob
}

function init_rococo_bulletin() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay init-bridge rococo-to-rococo-bulletin \
        --source-host localhost \
        --source-port 9942 \
        --source-version-mode Auto \
        --target-host localhost \
        --target-port 10000 \
        --target-version-mode Auto \
        --target-signer //Alice
}

function run_relay() {
    echo OK
    ensure_relayer

    RUST_LOG=rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay relay-headers-and-messages rococo-bulletin-bridge-hub-rococo \
        --rococo-bulletin-host localhost \
        --rococo-bulletin-port 10000 \
        --rococo-bulletin-version-mode Auto \
        --rococo-bulletin-signer //Alice \
        --rococo-bulletin-transactions-mortality 4 \
        --rococo-host localhost \
        --rococo-port 9942 \
        --bridge-hub-rococo-host localhost \
        --bridge-hub-rococo-port 8943 \
        --bridge-hub-rococo-version-mode Auto \
        --bridge-hub-rococo-signer //Charlie \
        --bridge-hub-rococo-transactions-mortality 4 \
        --lane 00000000
}

case "$1" in
  run-relay)
    init_bulletin_rococo
    init_rococo_bulletin
    run_relay
    ;;
  init-people-rococo-local)
      ensure_polkadot_js_api
      # HRMP
      open_hrmp_channels \
          "ws://127.0.0.1:9942" \
          "//Alice" \
          1004 1013 4 524288
      open_hrmp_channels \
          "ws://127.0.0.1:9942" \
          "//Alice" \
          1013 1004 4 524288
      ;;
  stop)
    pkill -f polkadot
    pkill -f polkadot-parachain
    pkill -f substrate-relay
    ;;
  *)
    echo "A command is require. Supported commands for:
    Local (zombienet) run:
          - run-relay";
    exit 1
    ;;
esac