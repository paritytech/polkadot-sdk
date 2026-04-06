#!/usr/bin/env bash
# Runs all smoldot E2E tests, retrying each until it passes.
#
# Environment variables:
#   MAX_RETRIES   - max attempts per test (default: 10)
#   RETRY_DELAY   - seconds between retries (default: 30)
#   SMOLDOT_JS_PATH - path to smoldot wasm-node/javascript dir
#   ZOMBIE_PROVIDER - zombienet provider (default: native)
#
# Usage:
#   PATH="$PWD/target/release:$PATH" \
#   SMOLDOT_JS_PATH="/path/to/smoldot/wasm-node/javascript" \
#   bash cumulus/zombienet/zombienet-sdk/tests/zombie_ci/run_smoldot_tests.sh

set -euo pipefail

MAX_RETRIES=${MAX_RETRIES:-10}
RETRY_DELAY=${RETRY_DELAY:-30}
export ZOMBIE_PROVIDER=${ZOMBIE_PROVIDER:-native}

if [ -z "${SMOLDOT_JS_PATH:-}" ]; then
    echo "ERROR: SMOLDOT_JS_PATH must be set to the smoldot wasm-node/javascript directory"
    echo "  e.g.: export SMOLDOT_JS_PATH=/path/to/smoldot/wasm-node/javascript"
    exit 1
fi
export SMOLDOT_JS_PATH

# Clean stale zombienet directories to prevent smoldot warp sync failures
# caused by connecting to chains with pre-existing high block numbers.
rm -rf /tmp/zombienet-smoldot-* 2>/dev/null || true

TESTS=(
    "statement_store_fullnode_to_smoldot"
    "statement_store_smoldot_submit"
    "statement_store_smoldot_to_smoldot"
    "statement_store_topic_filter_any"
    "statement_store_topic_filter_match_any"
    "statement_store_multi_topic_match_all"
    "statement_store_topic_filtering_negative"
    "statement_store_submit_invalid"
    "statement_store_unsubscribe"
    "statement_store_multiple_subscriptions"
    "statement_store_resubscribe_receives_again"
    "statement_store_initial_sync_all_filters"
)

FAILED=()

for test_name in "${TESTS[@]}"; do
    attempt=1
    passed=false
    while [ $attempt -le $MAX_RETRIES ]; do
        echo ""
        echo "========================================"
        echo "Running $test_name (attempt $attempt/$MAX_RETRIES)"
        echo "========================================"
        if cargo test -p cumulus-zombienet-sdk-tests --features zombie-ci \
            "$test_name" -- --nocapture 2>&1; then
            echo "PASSED: $test_name"
            passed=true
            break
        else
            echo "FAILED: $test_name (attempt $attempt)"
            if [ $attempt -lt $MAX_RETRIES ]; then
                echo "Retrying in ${RETRY_DELAY}s..."
                sleep $RETRY_DELAY
            fi
        fi
        attempt=$((attempt + 1))
    done
    if [ "$passed" = false ]; then
        echo "ERROR: $test_name failed after $MAX_RETRIES attempts"
        FAILED+=("$test_name")
    fi
done

echo ""
echo "========================================"
if [ ${#FAILED[@]} -eq 0 ]; then
    echo "ALL TESTS PASSED"
    exit 0
else
    echo "FAILED TESTS:"
    for t in "${FAILED[@]}"; do
        echo "  - $t"
    done
    exit 1
fi
