#!/usr/bin/env bash
# JAM collator demo: run polkadot-omni-node as a collator against a JAM network.
#
# This is a thin wrapper. Everything it used to do in bash — spin up the network, patch the
# chain spec, put the parasim service on the chain, launch the collators — now lives in the
# Rust harness at cumulus/zombienet/jam-tests, so the demo and the tests take exactly the same
# code path. The demo spawns its OWN JAM network, from a genesis carrying the parasim service
# and the para's core, so no testnet has to be running and nothing is bootstrapped afterwards.
#
# Prerequisites:
#   1. This repo built:
#        cargo build --release -p polkadot-omni-node -p parachain-template-runtime
#        # zombienet still requires a relay chain, and a relay validator needs its PVF workers
#        cargo build --release --bin polkadot \
#            --bin polkadot-prepare-worker --bin polkadot-execute-worker
#   2. A polkajam build whose `gen-spec` understands the `services` / `auth_queues` /
#      `assigners` config keys.
#   3. The parasim service and AURA authorizer blobs from the parachain-service repo.
#
# Required environment (see cumulus/zombienet/jam-tests/README.md):
#   JAM_NODE_BIN    path to the polkajam node binary
#   PARASIM_BLOB    path to parasim-service.jam
#   AUTHORIZER_BLOB path to parachain-authorizer-sr25519.jam
# Optional:
#   NUM_COLLATORS   how many collators to run (default 1)
#   OMNI_NODE_BIN, RUNTIME_WASM, RELAY_NODE_BIN   override the target/release defaults
#
# Usage:
#   JAM_NODE_BIN=... PARASIM_BLOB=... AUTHORIZER_BLOB=... cumulus/zombienet/jam-tests/demo.sh

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

exec cargo test -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --ignored --nocapture --test-threads 1 jam::demo
