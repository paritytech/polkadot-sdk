#!/usr/bin/env bash
set -euo pipefail

export RUSTFLAGS="${RUSTFLAGS:--Cdebug-assertions=y}"
export SKIP_WASM_BUILD=1

echo "RUSTFLAGS=${RUSTFLAGS}"
echo "SKIP_WASM_BUILD=${SKIP_WASM_BUILD}"
echo

cargo nextest run --release --workspace --no-fail-fast \
	--exclude 'asset-hub-*' \
	--exclude 'asset-test-utils' \
	--exclude 'assets-common' \
	--exclude 'bp-*' \
	--exclude 'bridge-hub-*' \
	--exclude 'bridge-runtime-common' \
	--exclude 'chain-spec-guide-runtime' \
	--exclude 'collectives-*' \
	--exclude 'coretime-*' \
	--exclude 'cumulus-*' \
	--exclude 'emulated-*' \
	--exclude 'glutton-*' \
	--exclude 'governance-*' \
	--exclude 'minimal-template-*' \
	--exclude 'pallet-bridge-*' \
	--exclude 'pallet-collator-selection' \
	--exclude 'pallet-collective-content' \
	--exclude 'pallet-minimal-template' \
	--exclude 'pallet-parachain-template' \
	--exclude 'pallet-xcm*' \
	--exclude 'parachain-template*' \
	--exclude 'parachains-*' \
	--exclude 'penpal-*' \
	--exclude 'people-*' \
	--exclude 'polkadot-*' \
	--exclude 'relay-*' \
	--exclude 'rococo-*' \
	--exclude 'snowbridge-*' \
	--exclude 'staging-parachain-info' \
	--exclude 'staging-xcm*' \
	--exclude 'substrate-relay-helper' \
	--exclude 'template-zombienet-*' \
	--exclude 'test-parachain-*' \
	--exclude 'test-runtime-constants' \
	--exclude 'testnet-*' \
	--exclude 'tracing-gum*' \
	--exclude 'westend-*' \
	--exclude 'xcm-*' \
	--exclude 'yet-another-*' \
	--exclude 'zombienet-*' \
	"$@"
