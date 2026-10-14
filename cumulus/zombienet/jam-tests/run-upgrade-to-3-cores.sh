#!/usr/bin/env bash
# C4 evidence run: the JAM branch of `elastic_scaling_upgrade_to_3_cores`.
#
# Everything the test shells out to is named explicitly, so the run is reproducible from the
# repository state alone. The runtime blobs are the riscv builds the jam-tests README prescribes.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

export LD_LIBRARY_PATH=/nix/store/jgda9w7ys57a5xjwbm179chz7d9sz937-clang-22.1.8-lib/lib:/nix/store/2ga5nd1m56n5cx2wh8vbf6nrdhqk2f0q-gcc-15.3.0-lib/lib:/nix/store/1pfrkdivg1xs06w2219z6fq3r97x0wxi-rocksdb-10.10.1/lib:$LD_LIBRARY_PATH

# The three blobs the harness needs: the parachain service, the sr25519 authorizer, the para's
# runtime. The para's is the elastic-scaling flavor — the one the test upgrades to.
export PARACHAIN_SERVICE_BLOB="$PWD/../parachain-service/target/pvm/riscv64emac-unknown-none-polkavm/production/parachain-service.jam"
export AUTHORIZER_BLOB="$PWD/../parachain-service/target/pvm/riscv64emac-unknown-none-polkavm/production-authorizer/parachain-authorizer-sr25519.jam"
export RUNTIME_WASM="$PWD/target/release/rbuild/cumulus-test-runtime/cumulus-test-runtime-blob.polkavm"
export JAM_NODE_BIN="$PWD/../polkajam2/target/release/polkajam"
export OMNI_NODE_BIN="$PWD/target/release/polkadot-omni-node"
# The Coretime chain the test spawns as para 1. The `fast-runtime` flavor: its broker assigns a
# force-reserved core within a 20-slot timeslice instead of 80.
export CORETIME_RUNTIME_WASM="$PWD/target/release/rbuild/coretime-westend-runtime/fast_runtime_binary.rs.polkavm"

export SUBSTRATE_RUNTIME_TARGET=riscv
export WASM_BUILD_RUSTFLAGS="${WASM_BUILD_RUSTFLAGS:-} --cfg jam"
export ZOMBIE_PROVIDER=native
export SUBSTRATE_ENABLE_POLKAVM=1
export POLKAVM_BACKEND="${POLKAVM_BACKEND:-interpreter}"

echo "PARACHAIN_SERVICE_BLOB=$PARACHAIN_SERVICE_BLOB"
echo "AUTHORIZER_BLOB=$AUTHORIZER_BLOB"
echo "RUNTIME_WASM=$RUNTIME_WASM"
echo "JAM_NODE_BIN=$JAM_NODE_BIN"
echo "CORETIME_RUNTIME_WASM=$CORETIME_RUNTIME_WASM"
for blob in "$PARACHAIN_SERVICE_BLOB" "$AUTHORIZER_BLOB" "$RUNTIME_WASM" "$JAM_NODE_BIN" "$CORETIME_RUNTIME_WASM" "$OMNI_NODE_BIN"; do
	if [[ ! -e $blob ]]; then
		echo "missing: $blob" >&2
		exit 2
	fi
done

exec cargo test -p cumulus-zombienet-sdk-tests --features jam,zombie-ci --test tests \
	-- --test-threads 1 --nocapture "$@"