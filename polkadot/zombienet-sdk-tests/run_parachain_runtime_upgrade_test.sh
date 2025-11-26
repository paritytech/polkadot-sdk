#!/usr/bin/env bash

# Скрипт для локального запуска теста parachain_runtime_upgrade
# Требует сборки test-parachain и polkadot бинарников, а также WASM рантайма с slot duration 18s

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="${SCRIPT_DIR}/../.."

echo "=== Building test-parachain ==="
cd "${WORKSPACE_ROOT}"
cargo build --release -p cumulus-test-service --bin test-parachain -j 6

echo ""
echo "=== Copying WASM runtime to /tmp ==="
WASM_SOURCE="${WORKSPACE_ROOT}/target/release/wbuild/cumulus-test-runtime/wasm_binary_slot_duration_18s.rs.compact.compressed.wasm"
if [ -f "${WASM_SOURCE}" ]; then
    cp "${WASM_SOURCE}" /tmp/
    echo "WASM runtime copied successfully: ${WASM_SOURCE} -> /tmp/"
else
    echo "ERROR: WASM runtime not found at ${WASM_SOURCE}"
    echo "The runtime should be built automatically during test-parachain build."
    exit 1
fi

echo ""
echo "=== Building polkadot and workers ==="
cargo build --release --features fast-runtime -p polkadot --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker -j 6

echo ""
echo "=== Running test ==="
cd "${SCRIPT_DIR}"
export PATH="${WORKSPACE_ROOT}/target/release:${PATH}"
ZOMBIE_PROVIDER=native cargo test --release --features zombie-ci --test lib parachain_runtime_upgrade_test -- --nocapture --test-threads=1

echo ""
echo "=== Test completed ==="

