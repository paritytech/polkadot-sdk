#!/usr/bin/env bash
# JAM collator demo: run polkadot-omni-node as a collator against a JAM network.
#
# This is a thin wrapper. Everything it used to do in bash — spin up the network, patch the
# chain spec, register the parasim service, launch the collators — now lives in the Rust
# harness at cumulus/zombienet/jam-tests, so the demo and the tests take exactly the same
# code path. The demo spawns its OWN JAM network, so no testnet has to be running.
#
# Prerequisites:
#   1. This repo built:
#        cargo build --release -p polkadot-omni-node -p parachain-template-runtime
#        cargo build --release --bin polkadot   # zombienet still requires a relay chain
#   2. A polkajam build (the `polkajam` node binary and the `jamt` CLI).
#   3. The parasim service blob from the parachain-service repo.
#
# Required environment (see cumulus/zombienet/jam-tests/README.md):
#   JAM_NODE_BIN    path to the polkajam node binary
#   JAMT_BIN        path to the jamt CLI
#   PARASIM_BLOB    path to parasim-service.jam
# Optional:
#   NUM_COLLATORS   how many collators to run (default 1)
#   OMNI_NODE_BIN, RUNTIME_WASM, RELAY_NODE_BIN   override the target/release defaults
#
# Usage:
#   JAM_NODE_BIN=... JAMT_BIN=... PARASIM_BLOB=... cumulus/scripts/jam-collator-demo.sh

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

exec cargo test -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --ignored --nocapture --test-threads 1 jam::demo
