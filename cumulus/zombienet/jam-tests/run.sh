#!/usr/bin/env bash
# JAM collator tests: build the PolkaVM runtime with the `jam` cfg and run the suite.
#
# The JAM-gated code in cumulus-pallet-parachain-system (`jam_implementation`, the
# `jam_validate_block` entry, the `[target.'cfg(jam)'.dependencies]`) is compiled into the
# runtime blob only when the build sets `--cfg jam`. That cfg is a test-launch concern, so this
# script injects it via `WASM_BUILD_RUSTFLAGS`, which the wasm-builder appends to the blob
# build's RUSTFLAGS (`--cfg substrate_runtime --cfg jam` for the riscv blob), then runs the
# suite against the blob it just built.
#
# Prerequisites (see README.md):
#   - a polkajam build whose `gen-spec` understands the `services` / `auth_queues` /
#     `assigners` config keys and whose RPC serves `stateValue`.
#   - this repo's node binaries: `cargo build --release -p polkadot-omni-node` plus
#     `--bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker`.
#   - the parachain-service `.jam` blobs from the parachain-service repo.
#
# Required environment (see README.md):
#   JAM_NODE_BIN            path to the polkajam node binary
#   PARACHAIN_SERVICE_BLOB  path to parachain-service.jam
#   AUTHORIZER_BLOB         path to parachain-authorizer-sr25519.jam
# Optional:
#   JAM_GENSPEC_BIN         the polkajam build that runs gen-spec, if not JAM_NODE_BIN
#   PARASIM_TOOL_BIN        for the two dynamic-core tests
#   NUM_COLLATORS           how many collators to run (default 1)
#   OMNI_NODE_BIN, RELAY_NODE_BIN, RUNTIME_WASM   override the target/release defaults
#
# Usage:
#   JAM_NODE_BIN=... PARACHAIN_SERVICE_BLOB=... AUTHORIZER_BLOB=... \
#     cumulus/zombienet/jam-tests/run.sh [test-filter]
#
#   The test filter defaults to `jam::collator_progress`; pass any other filter as $1
#   (e.g. `jam::demo` or `jam::core_assignment`).

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

# Build the runtime as the JAM (riscv/PolkaVM) blob with `--cfg jam` injected through the
# wasm-builder's RUSTFLAGS channel. Scoped to this command so the cfg never leaks elsewhere.
SUBSTRATE_RUNTIME_TARGET=riscv \
WASM_BUILD_RUSTFLAGS="${WASM_BUILD_RUSTFLAGS:-} --cfg jam" \
cargo build --release -p parachain-template-runtime

# The harness expects the blob under `RUNTIME_WASM`; default it to the just-built PolkaVM blob.
export RUNTIME_WASM="${RUNTIME_WASM:-$PWD/target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm}"

exec cargo test -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --test-threads 1 --nocapture "${1:-jam::collator_progress}"