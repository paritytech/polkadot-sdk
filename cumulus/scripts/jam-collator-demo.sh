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
#        jamt --force-core 0 create-service <parasim-service.jam> 1000000000000000 \
#            --register=parasim --raw --id 5
#      Register from a COPY of the blob: PVM builds are not byte-deterministic and a later
#      cargo run can rewrite the blob after its hash was registered, leaving the service
#      without a resolvable code preimage ("Service code not found").
#      Alternatively set JAMT_BIN + PARASIM_BLOB below and this script registers a fresh
#      service (from a copy) on every run, which is what the automated tests use.
#   3. The AURA authorizer blob and the parasim-tool binary, from the same repo:
#        cargo build --release -p parachain-authorizer-bin -p parasim-tool
#      This script hosts the blob on the chain and points a core at it; see step 1 below.
#   4. This repo built: cargo build --release -p polkadot-omni-node -p parachain-template-runtime
#
# The demo parachain uses PARA ID 0. That id is what `parasim-tool assign-core 0 <core>`
# writes into the authorizer config the core commits to, and what the chain spec below
# pins, so the two have to stay in step.
#
# The core is NOT named to the collator: it computes its para's authorizer hash from
# AUTHORIZER_BLOB plus the collator set its runtime names, and scans the authorizer pools
# for it.
#
# Usage: JAM_RPC=ws://127.0.0.1:19800 JAM_SERVICE_ID=5 cumulus/scripts/jam-collator-demo.sh
#
# Required environment:
#   AUTHORIZER_BLOB   path to parachain-authorizer.jam (the AURA authorizer the core runs)
#   PARASIM_TOOL_BIN  path to the parasim-tool binary (deploy, assign, grant)
#
# Optional environment (all default to the single-collator demo behaviour):
#   NUM_COLLATORS   1..6; runs --alice, --bob, --charlie, --dave, --eve, --ferdie (default 1)
#   DAEMONIZE       1 to background the collators even when NUM_COLLATORS=1
#   WORK_DIR        state directory; re-usable across runs (restart testing)
#   JAM_RPC         JAM node JSON-RPC endpoint
#   JAM_SERVICE_ID  parasim service to collate for; if unset and JAMT_BIN + PARASIM_BLOB
#                   are set, a fresh service is registered and its id echoed as
#                   `JAM_SERVICE_ID=<id>` (this is what isolates concurrent/sequential runs
#                   sharing one testnet: each service has its own para-0 head)
#   JAM_ASSIGN_CORE core to point at para 0 (default 0). Its queue is rewritten, so pick one
#                   nothing else on the testnet is using.
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
JAM_ASSIGN_CORE="${JAM_ASSIGN_CORE:-0}"
NUM_COLLATORS="${NUM_COLLATORS:-1}"
DAEMONIZE="${DAEMONIZE:-0}"
P2P_PORT="${P2P_PORT:-30333}"
RPC_PORT="${RPC_PORT:-9944}"
WORK_DIR="${WORK_DIR:-$(mktemp -d /tmp/jam-collator-demo.XXXXXX)}"
mkdir -p "$WORK_DIR"

OMNI_NODE="$ROOT/target/release/polkadot-omni-node"
RUNTIME_WASM="$(ls "$ROOT"/target/release/wbuild/parachain-template-runtime/parachain_template_runtime.compact.compressed.wasm)"
SPEC="$WORK_DIR/jam-parachain-spec.json"

for var in AUTHORIZER_BLOB PARASIM_TOOL_BIN; do
	if [[ -z "${!var:-}" ]]; then
		echo "$var must be set; see the header of this script" >&2
		exit 1
	fi
done

# The dev accounts omni-node accepts as `--<name>`, in aura slot order.
COLLATORS=(alice bob charlie dave eve ferdie)
if (( NUM_COLLATORS < 1 || NUM_COLLATORS > ${#COLLATORS[@]} )); then
	echo "NUM_COLLATORS must be between 1 and ${#COLLATORS[@]}" >&2
	exit 1
fi
# The collator set as `parasim-tool --collators` spells it: the position of a name in this
# list is the collator index the authorizer hash commits to, and it is the same list the
# chain spec's aura authorities are built from, which is where the collators read it.
COLLATOR_NAMES="$(IFS=,; echo "${COLLATORS[*]:0:NUM_COLLATORS}")"

# Collator 0 keeps the original path so an existing WORK_DIR still restarts the demo.
collator_base_path() {
	if (( $1 == 0 )); then echo "$WORK_DIR/collator"; else echo "$WORK_DIR/collator-${COLLATORS[$1]}"; fi
}

# 0. The parasim service. A fresh one per run keeps runs that share a testnet from
#    colliding on para 0's head. Register a COPY: PVM builds are not byte-deterministic,
#    so a rebuild of the original blob would strand the registered code hash.
#
#    `--force-core 0`: jamt picks a core at random otherwise, and it builds its packages
#    under the genesis authorizer, so a core already pointed at a para refuses them. This
#    is the only jamt call here and it runs before step 1 assigns anything; if the testnet
#    already has core 0 assigned from an earlier run, name a free core with --force-core.
if [[ -z "${JAM_SERVICE_ID:-}" && -n "${JAMT_BIN:-}" && -n "${PARASIM_BLOB:-}" ]]; then
	cp "$PARASIM_BLOB" "$WORK_DIR/parasim-service.jam"
	for _ in 1 2 3 4 5; do
		# `jamt --id` must be unused and below 65536; retry to survive a random collision.
		if JAM_SERVICE_ID="$("$JAMT_BIN" --rpc "$JAM_RPC" --force-core 0 create-service \
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

# 1. The AURA authorizer, and the core that runs it. Three steps, in this order:
#
#      deploy-authorizer  hosts the blob in the bootstrap service (solicit, then provide).
#                         Validators fetch authorizer code by preimage lookup, so a core
#                         pointed at a hash nobody hosts authorizes nothing, silently.
#      assign-core        points the core's queue at para 0's AURA authorizer. Service 0 is
#                         the assigner of every core at genesis and a bootstrap instruction
#                         only rides a core still holding the genesis authorizer, so this
#                         rides the very core it assigns — hence it comes before the grant.
#      grant-assigner     hands the core's assigner privilege to parasim, which is what
#                         lets a later free-core or re-assignment ride an AURA package's
#                         token. It is a bootstrap instruction too, so it needs *another*
#                         core still holding the genesis authorizer.
#
#    Deploy from a COPY, for the same reason the service blob is copied: the authorizer
#    hash is a hash of exactly these bytes, and the collators below are given this file.
AUTHORIZER_COPY="$WORK_DIR/parachain-authorizer.jam"
cp "$AUTHORIZER_BLOB" "$AUTHORIZER_COPY"

parasim_tool() {
	# --collators and --authorizer-blob are what the authorizer hash is built from, so they
	# have to be exactly what the collators are started with below.
	"$PARASIM_TOOL_BIN" --rpc "$JAM_RPC" --service "$JAM_SERVICE_ID" \
		--authorizer-blob "$AUTHORIZER_COPY" --collators "$COLLATOR_NAMES" "$@"
}

echo "deploying the AURA authorizer $AUTHORIZER_COPY"
parasim_tool deploy-authorizer
echo "assigning core $JAM_ASSIGN_CORE to para 0 for $COLLATOR_NAMES"
parasim_tool assign-core 0 "$JAM_ASSIGN_CORE"
echo "granting core $JAM_ASSIGN_CORE's assigner privilege to service $JAM_SERVICE_ID"
parasim_tool grant-assigner "$JAM_ASSIGN_CORE"

# 2. Chain spec: template runtime, dev preset, para id 0, JAM marker.
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

# 3. The node network key, one per collator: a collator is an authority and refuses to
#    auto-generate one. Generation fails if the key exists, so a re-run with the same
#    WORK_DIR (restart testing) must skip it. The key the collator signs work packages with
#    is its aura key, which `--alice` and friends put in the keystore in memory.
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
		--jam-authorizer-blob "$AUTHORIZER_COPY"
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

# 4. The collator(s). Watch the logs (target 'jam-collator' and 'jam-rpc-interface'):
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
