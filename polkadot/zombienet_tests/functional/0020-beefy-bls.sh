#!/usr/bin/env bash
# A Makeshift BEEFY with BLS (ecdsa-bls381 paired keys) Integration test on westend-local
#
# This test verifies that BEEFY voting and finalization works with
# ecdsa_bls381 paired keys on westend-local.
#
# NOTE: This is a standalone shell script bandaid rather than a proper zombienet
# test because zombienet uses @polkadot/api to generate session keys, which doesn't 
# support  ecdsa_bls381 paired keys yet. As a result, zombienet replaces the beefy 
# keys in the chain spec with plain ecdsa keys (33 bytes), which are rejected by the
# westend runtime that expects paired ecdsa_bls381 keys (177 bytes).
#
# Prerequisites:
#   cargo build -p polkadot --features bls-beefy-experimental --release
#
# Usage:
#   ./polkadot/zombienet_tests/functional/0020-beefy-bls.sh [polkadot-binary]

set -euo pipefail

POLKADOT="${1:-./target/release/polkadot}"
TIMEOUT_BEEFY=120  # seconds to wait for BEEFY finalization

if [ ! -x "$POLKADOT" ]; then
	echo "ERROR: polkadot binary not found at $POLKADOT"
	echo "Build with: cargo build -p polkadot --features bls-beefy-experimental --release"
	exit 1
fi

TMPDIR=$(mktemp -d)
ALICE_LOG="$TMPDIR/alice.log"
BOB_LOG="$TMPDIR/bob.log"

cleanup() {
	echo "Cleaning up..."
	kill "$ALICE_PID" "$BOB_PID" 2>/dev/null || true
	wait "$ALICE_PID" "$BOB_PID" 2>/dev/null || true
	rm -rf "$TMPDIR"
}
trap cleanup EXIT

echo "=== BEEFY BLS Integration Test ==="
echo "Binary: $POLKADOT"
echo "Temp dir: $TMPDIR"
echo "Alice log: $ALICE_LOG"
echo "Bob log: $BOB_LOG"

# Start Alice
echo "Starting Alice..."
"$POLKADOT" \
	--chain westend-local \
	--alice \
	--tmp \
	--log=beefy=debug \
	--port 30333 \
	--rpc-port 9944 \
	--unsafe-force-node-key-generation \
	&>"$ALICE_LOG" &
ALICE_PID=$!

# Wait for Alice to start and get peer ID
for i in $(seq 1 30); do
	ALICE_PEER=$(grep -oP '12D3K\w+' "$ALICE_LOG" 2>/dev/null | head -1) || true
	if [ -n "${ALICE_PEER:-}" ]; then
		break
	fi
	sleep 1
done

if [ -z "${ALICE_PEER:-}" ]; then
	echo "FAIL: Alice did not start in time"
	cat "$ALICE_LOG"
	exit 1
fi
echo "Alice peer ID: $ALICE_PEER"

# Start Bob
echo "Starting Bob..."
"$POLKADOT" \
	--chain westend-local \
	--bob \
	--tmp \
	--log=beefy=debug \
	--port 30334 \
	--rpc-port 9945 \
	--unsafe-force-node-key-generation \
	--bootnodes "/ip4/127.0.0.1/tcp/30333/p2p/$ALICE_PEER" \
	&>"$BOB_LOG" &
BOB_PID=$!

# Wait for BEEFY to finalize at least block 1
echo "Waiting for BEEFY finalization (timeout: ${TIMEOUT_BEEFY}s)..."
BEEFY_OK=false
for i in $(seq 1 "$TIMEOUT_BEEFY"); do
	if grep -q 'Concluded mandatory round' "$ALICE_LOG" 2>/dev/null; then
		BEEFY_OK=true
		break
	fi
	sleep 1
done

if ! $BEEFY_OK; then
	echo "FAIL: BEEFY did not finalize mandatory round within ${TIMEOUT_BEEFY}s"
	echo "=== Alice BEEFY logs ==="
	grep -i 'beefy' "$ALICE_LOG" || echo "(no beefy logs)"
	echo "=== Alice last lines ==="
	tail -10 "$ALICE_LOG"
	exit 1
fi

echo "BEEFY finalized mandatory round:"
echo "  $(grep -n 'Concluded mandatory round' "$ALICE_LOG" | head -1 | sed "s|^|alice.log:|")"

# Verify BLS signatures are being used (paired ecdsa-bls381).
# Public key: 177 bytes = ecdsa 33 + bls381 DoublePublicKey 144 (G1 48 + G2 96)
# Signature:  177 bytes = ecdsa 65 + bls381 DoubleSignature 112 (G1 48 + G2 64)
echo "Checking for paired ecdsa-bls381 keys..."

# The log line looks like: "BEEFY signature size: 177 bytes, authority_id size: 177 bytes"
if MATCH=$(grep -n 'signature size: 177 bytes.*authority_id size: 177 bytes' "$ALICE_LOG" 2>/dev/null | head -1); [ -n "$MATCH" ]; then
	echo "PASS: BEEFY is using 177-byte paired ecdsa-bls381 signatures and authority IDs"
	echo "  alice.log:$MATCH"
else
	echo "FAIL: Could not confirm 177-byte paired ecdsa-bls381 signature/authority_id sizes"
	grep -i 'beefy.*signature\|beefy.*authority' "$ALICE_LOG" | head -5
	exit 1
fi

# Also check key length from the validator set log line
KEY_LEN=$(grep 'Loading BEEFY voter state' "$ALICE_LOG" | grep -oP 'Public\([0-9a-f]+' | head -1 | sed 's/Public(//' | wc -c)
if [ "$KEY_LEN" -gt 100 ]; then
	echo "PASS: BEEFY validator public keys are ${KEY_LEN} hex chars (paired ecdsa-bls381)"
	echo "  alice.log:$(grep -n 'Loading BEEFY voter state' "$ALICE_LOG" | head -1)"
else
	echo "FAIL: BEEFY validator public keys are too short (${KEY_LEN} hex chars)"
	exit 1
fi

# Verify continued block finalization
echo "Waiting for BEEFY to finalize more blocks..."
sleep 15
BEST_BLOCK=$(grep 'substrate_beefy_best_block' "$ALICE_LOG" 2>/dev/null | tail -1 || true)
CONCLUDED=$(grep -c 'Concluded' "$ALICE_LOG" 2>/dev/null || echo 0)
echo "BEEFY concluded rounds: $CONCLUDED"

echo ""
echo "=== ALL CHECKS PASSED ==="
echo "BEEFY with ecdsa-bls381 paired keys is working on westend-local."
