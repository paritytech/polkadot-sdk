#!/usr/bin/env bash
# JAM collator tests: build the PolkaVM runtime with the `jam` cfg and run a suite.
#
# The JAM-gated code in cumulus-pallet-parachain-system (`jam_implementation`, the
# `jam_validate_block` entry, the `[target.'cfg(jam)'.dependencies]`) is compiled into the
# runtime blob only when the build sets `--cfg jam`. That cfg is a test-launch concern, so this
# script injects it via `WASM_BUILD_RUSTFLAGS`, which the wasm-builder appends to the blob
# build's RUSTFLAGS (`--cfg substrate_runtime --cfg jam` for the riscv blob), then runs the
# selected suite against the blob it just built.
#
# The `jam`, `elastic-scaling`, `block-bundling` and `resubmission` suites live in
# `cumulus-zombienet-sdk-tests` and need the `jam` feature on their own build: the feature selects
# the JAM `zombienet-sdk` and the `cumulus-jam-zombienet-tests` helper library. It is scoped to
# the one cargo invocation that needs it, so the blob build above stays as it was.
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
#     cumulus/zombienet/jam-tests/run.sh [--suite <jam|elastic-scaling|block-bundling|resubmission>] \
#     [test-filter]
#
#   --suite jam (the default) runs the JAM suite of `cumulus-zombienet-sdk-tests`
#   (`tests/jam/mod.rs`). The optional test filter defaults to `jam::collator_progress`; pass any
#   other filter (e.g. `jam::demo` or `jam::core_assignment`).
#
#   --suite elastic-scaling and --suite block-bundling run the matching suite of
#   `cumulus-zombienet-sdk-tests` under `--cfg jam`; their filter is fixed and the positional
#   test filter is not used.
#
#   --suite resubmission runs `jam::resubmission` of `cumulus-zombienet-sdk-tests` — the
#   dropping-proxy resend test.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

suite=jam
filter=""
while [[ $# -gt 0 ]]; do
	case "$1" in
	--suite)
		suite="${2:?--suite needs a value: jam, elastic-scaling, block-bundling or resubmission}"
		shift 2
		;;
	--suite=*)
		suite="${1#--suite=}"
		shift
		;;
	*)
		filter="$1"
		shift
		;;
	esac
done

case "$suite" in
jam | elastic-scaling | block-bundling | resubmission) ;;
*)
	echo "unknown suite: $suite (expected jam, elastic-scaling, block-bundling or resubmission)" >&2
	exit 2
	;;
esac

# Build the runtime as the JAM (riscv/PolkaVM) blob with `--cfg jam` injected through the
# wasm-builder's RUSTFLAGS channel. Scoped to this command so the cfg never leaks elsewhere.
SUBSTRATE_RUNTIME_TARGET=riscv \
WASM_BUILD_RUSTFLAGS="${WASM_BUILD_RUSTFLAGS:-} --cfg jam" \
cargo build --release -p parachain-template-runtime

# The harness expects the blob under `RUNTIME_WASM`; default it to the just-built PolkaVM blob.
export RUNTIME_WASM="${RUNTIME_WASM:-$PWD/target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm}"

case "$suite" in
jam | elastic-scaling | block-bundling | resubmission)
	case "$suite" in
	elastic-scaling) test_filter=zombie_ci::elastic_scaling ;;
	block-bundling) test_filter=zombie_ci::block_bundling ;;
	resubmission) test_filter=jam::resubmission ;;
	jam) test_filter="${filter:-jam::collator_progress}" ;;
	esac
	# The sdk test crate's JAM paths are selected by the `jam` feature, which type-checks them
	# without a whole-workspace rebuild. The feature is scoped to the one cargo invocation that
	# needs it.
	#
	# The crate embeds `cumulus-test-runtime` feature flavors. Under the riscv target the
	# wasm-builder emits each flavor's PolkaVM blob under the same `WASM_BINARY` const, so the
	# build must run (no `SKIP_WASM_BUILD`) with `--cfg jam` injected through its RUSTFLAGS
	# channel. This is a heavy build: every flavor is compiled for riscv.
	export SUBSTRATE_RUNTIME_TARGET=riscv
	export WASM_BUILD_RUSTFLAGS="${WASM_BUILD_RUSTFLAGS:-} --cfg jam"
	# zombienet-sdk supports JAM on the native provider only; the default would be docker.
	export ZOMBIE_PROVIDER="${ZOMBIE_PROVIDER:-native}"
	# zombienet converts each para's chain spec to raw by invoking the parachain command itself,
	# and that invocation inherits this process's environment rather than the harness's per-command
	# one — without these it rejects the PolkaVM runtime blob.
	export SUBSTRATE_ENABLE_POLKAVM=1
	export POLKAVM_BACKEND="${POLKAVM_BACKEND:-interpreter}"
	export POLKAVM_ALLOW_INSECURE=1
	exec cargo test -p cumulus-zombienet-sdk-tests --features jam,zombie-ci --test tests \
		-- --test-threads 1 --nocapture "$test_filter"
	;;
esac
