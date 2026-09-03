#!/usr/bin/env bash
# Phase-1 JAM collator demo: run polkadot-omni-node as a collator against a
# local polkajam testnet with the parasim service (spec B.0, phases 1-2).
#
# This script does NOT start a JAM network. It assumes one is already running and reachable
# at JAM_RPC (default ws://127.0.0.1:19800), with the parasim service registered on it: either
# an existing service pinned with JAM_SERVICE_ID (default 5), or a fresh one this script
# registers itself when JAMT_BIN and PARASIM_BLOB are set. Sequential runs against one testnet
# must not share a service id -- see JAM_SERVICE_ID below.
#
# For a self-contained run that spawns its own JAM network with zombienet instead, use
# cumulus/zombienet/jam-tests/demo.sh.
#
# Prerequisites (see docs/cumulus-jam-components/07-integration.md):
#   1. A polkajam testnet with the JIP-2 stateValue/stateProof extension:
#        polkajam-testnet --num-ordinary-nodes 1        # RPC on ws://127.0.0.1:19800
#      (In sandboxes without userfaultfd: POLKAVM_BACKEND=interpreter POLKAVM_ALLOW_INSECURE=1.)
#   2. The parasim service registered on it (parachain-service repo):
#        jamt create-service <parasim-service.jam> 1000000000000000 \
#            --register=parasim --raw --id 5
#      Register from a COPY of the blob: PVM builds are not byte-deterministic and a later
#      cargo run can rewrite the blob after its hash was registered, leaving the service
#      without a resolvable code preimage ("Service code not found").
#      Alternatively set JAMT_BIN + PARASIM_BLOB below and this script registers a fresh
#      service (from a copy) on every run, which is what the automated tests use.
#   3. This repo built: cargo build --release -p polkadot-omni-node -p parachain-template-runtime
#
# The demo parachain uses PARA ID 0: under the dev-genesis null authorizer (empty
# config) parasim falls back to ParaId(0), so the collator must build, submit and
# watch para 0 for the loop to close.
#
# Usage: JAM_RPC=ws://127.0.0.1:19800 JAM_SERVICE_ID=5 cumulus/scripts/jam-collator-demo.sh
#
# Environment (all default to the single-collator demo behaviour):
#   NUM_COLLATORS   1..6; runs --alice, --bob, --charlie, --dave, --eve, --ferdie (default 1)
#   DAEMONIZE       1 to background the collators even when NUM_COLLATORS=1
#   WORK_DIR        state directory; re-usable across runs (restart testing)
#   JAM_RPC         JAM node JSON-RPC endpoint
#   JAM_SERVICE_ID  parasim service to collate for; if unset and JAMT_BIN + PARASIM_BLOB
#                   are set, a fresh service is registered and its id echoed as
#                   `JAM_SERVICE_ID=<id>` (this is what isolates concurrent/sequential runs
#                   sharing one testnet: each service has its own para-0 head)
#   JAM_CORE        core to submit work packages to
#   JAMT_BIN        path to the polkajam `jamt` binary (fresh-service registration)
#   PARASIM_BLOB    path to parasim-service.jam (fresh-service registration)
#   P2P_PORT        base p2p port, collator i listens on P2P_PORT+i
#   RPC_PORT        base JSON-RPC port, collator i listens on RPC_PORT+i
#
# With more than one collator the script stays in the foreground supervising the children:
# PIDs are written to WORK_DIR/pids, logs to WORK_DIR/logs/<name>.log, and SIGTERM/SIGINT
# tears the whole set down.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
JAM_RPC="${JAM_RPC:-ws://127.0.0.1:19800}"
JAM_CORE="${JAM_CORE:-0}"
NUM_COLLATORS="${NUM_COLLATORS:-1}"
DAEMONIZE="${DAEMONIZE:-0}"
P2P_PORT="${P2P_PORT:-30333}"
RPC_PORT="${RPC_PORT:-9944}"
WORK_DIR="${WORK_DIR:-$(mktemp -d /tmp/jam-collator-demo.XXXXXX)}"
mkdir -p "$WORK_DIR"

OMNI_NODE="$ROOT/target/release/polkadot-omni-node"
RUNTIME_WASM="$(ls "$ROOT"/target/release/wbuild/parachain-template-runtime/parachain_template_runtime.compact.compressed.wasm)"
SPEC="$WORK_DIR/jam-parachain-spec.json"

# The dev accounts omni-node accepts as `--<name>`, in aura slot order.
COLLATORS=(alice bob charlie dave eve ferdie)
if (( NUM_COLLATORS < 1 || NUM_COLLATORS > ${#COLLATORS[@]} )); then
	echo "NUM_COLLATORS must be between 1 and ${#COLLATORS[@]}" >&2
	exit 1
fi

# Collator 0 keeps the original path so an existing WORK_DIR still restarts the demo.
collator_base_path() {
	if (( $1 == 0 )); then echo "$WORK_DIR/collator"; else echo "$WORK_DIR/collator-${COLLATORS[$1]}"; fi
}

# 0. The parasim service. A fresh one per run keeps runs that share a testnet from
#    colliding on para 0's head. Register a COPY: PVM builds are not byte-deterministic,
#    so a rebuild of the original blob would strand the registered code hash.
if [[ -z "${JAM_SERVICE_ID:-}" && -n "${JAMT_BIN:-}" && -n "${PARASIM_BLOB:-}" ]]; then
	cp "$PARASIM_BLOB" "$WORK_DIR/parasim-service.jam"
	for _ in 1 2 3 4 5; do
		# `jamt --id` must be unused and below 65536; retry to survive a random collision.
		if JAM_SERVICE_ID="$("$JAMT_BIN" --rpc "$JAM_RPC" create-service \
			"$WORK_DIR/parasim-service.jam" 1000000000000000 \
			--register=parasim --raw --id "$((100 + RANDOM % 10000))")"; then
			break
		fi
	done
	if [[ -z "${JAM_SERVICE_ID:-}" ]]; then
		echo "failed to register a parasim service on $JAM_RPC" >&2
		exit 1
	fi
	# `jamt --raw` prints the id as eight hex digits, omni-node wants it in decimal.
	JAM_SERVICE_ID="$((16#$JAM_SERVICE_ID))"
fi
JAM_SERVICE_ID="${JAM_SERVICE_ID:-5}"
echo "JAM_SERVICE_ID=$JAM_SERVICE_ID"

# 1. Chain spec: template runtime, dev preset, para id 0, JAM marker.
"$OMNI_NODE" chain-spec-builder --chain-spec-path "$SPEC" \
	create --relay-chain jam --para-id 0 -r "$RUNTIME_WASM" named-preset development

# The collators only alternate on their 6s slots if the genesis authority set is exactly the
# set of running collators, so derive the first NUM_COLLATORS dev keys for the patch below.
AURA_KEYS=()
for ((i = 0; i < NUM_COLLATORS; i++)); do
	name="${COLLATORS[i]}"
	AURA_KEYS+=("$("$OMNI_NODE" key inspect --scheme sr25519 "//${name^}" |
		awk '/SS58 Address:/ { print $3 }')")
done

python3 - "$SPEC" "${AURA_KEYS[@]}" <<'EOF'
import json, sys
path, authorities = sys.argv[1], sys.argv[2:]
spec = json.load(open(path))
# The preset pins para id 1000; the JAM demo needs para 0 (parasim's null-authorizer fallback).
patch = spec["genesis"]["runtimeGenesis"]["patch"]
patch.setdefault("parachainInfo", {})["parachainId"] = 0
# The template runtime has no `aura.authorities` in its presets: pallet-session drives
# pallet-aura, so the authority set is the session keys of the invulnerable collators.
patch["collatorSelection"]["invulnerables"] = authorities
patch["session"]["keys"] = [[key, key, {"aura": key}] for key in authorities]
spec["para_id"] = 0
spec["relay_chain"] = "jam"
json.dump(spec, open(path, "w"), indent=2)
EOF
echo "chain spec: $SPEC"

# 2. A stable node network key per collator (a collator is an authority; it refuses to
#    auto-generate one). Generation fails if the key exists, so a re-run with the
#    same WORK_DIR (restart testing) must skip it.
for ((i = 0; i < NUM_COLLATORS; i++)); do
	base_path="$(collator_base_path "$i")"
	if ! ls "$base_path"/chains/*/network/secret_* >/dev/null 2>&1; then
		"$OMNI_NODE" key generate-node-key --base-path "$base_path" --chain "$SPEC" 2>/dev/null
	fi
done

# The keys are hex on disk, so collator 0's peer id is known before anything is launched.
BOOTNODE_KEY="$(ls "$(collator_base_path 0)"/chains/*/network/secret_ed25519)"
BOOTNODE="/ip4/127.0.0.1/tcp/$P2P_PORT/p2p/$("$OMNI_NODE" key inspect-node-key --file "$BOOTNODE_KEY")"

collator_args() {
	local index="$1"
	local args=(
		--chain "$SPEC"
		--collator "--${COLLATORS[$index]}"
		--jam-rpc-urls "$JAM_RPC"
		--jam-service-id "$JAM_SERVICE_ID"
		--jam-core "$JAM_CORE"
		--base-path "$(collator_base_path "$index")"
		--port "$((P2P_PORT + index))"
		--rpc-port "$((RPC_PORT + index))"
		--force-authoring
		-l jam-collator=debug,jam-rpc-interface=debug
	)
	if (( index > 0 )); then
		args+=(--no-prometheus --bootnodes "$BOOTNODE")
	fi
	printf '%s\n' "${args[@]}"
}

# 3. The collator(s). Watch the logs (target 'jam-collator' and 'jam-rpc-interface'):
#    JAM best/finalized blocks tick, blocks get built, work packages submitted,
#    status reaches Reported, and the para head advances in JAM state.
if (( NUM_COLLATORS == 1 )) && [[ "$DAEMONIZE" != "1" ]]; then
	mapfile -t args < <(collator_args 0)
	exec "$OMNI_NODE" "${args[@]}"
fi

mkdir -p "$WORK_DIR/logs"
PIDS=()
trap 'kill "${PIDS[@]}" 2>/dev/null || true' EXIT INT TERM
for ((i = 0; i < NUM_COLLATORS; i++)); do
	mapfile -t args < <(collator_args "$i")
	log="$WORK_DIR/logs/${COLLATORS[i]}.log"
	"$OMNI_NODE" "${args[@]}" >"$log" 2>&1 &
	PIDS+=("$!")
	echo "collator ${COLLATORS[i]}: pid $! rpc 127.0.0.1:$((RPC_PORT + i)) log $log"
done
printf '%s\n' "${PIDS[@]}" >"$WORK_DIR/pids"
wait
