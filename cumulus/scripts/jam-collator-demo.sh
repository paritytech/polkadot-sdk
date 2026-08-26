#!/usr/bin/env bash
# Phase-1 JAM collator demo: run polkadot-omni-node as a collator against a
# local polkajam testnet with the parasim service (spec B.0, phases 1-2).
#
# Prerequisites (see docs/cumulus-jam-components/07-integration.md):
#   1. A polkajam testnet with the JIP-2 stateValue/stateProof extension:
#        polkajam-testnet --num-ordinary-nodes 1        # RPC on ws://127.0.0.1:19800
#      (In sandboxes without userfaultfd: POLKAVM_BACKEND=interpreter POLKAVM_ALLOW_INSECURE=1.)
#   2. The parasim service registered on it (parachain-service repo):
#        jamt create-service <parasim-service.jam> 1000000000000000000 \
#            --register=parasim --raw --id 5
#      Register from a COPY of the blob: PVM builds are not byte-deterministic and a later
#      cargo run can rewrite the blob after its hash was registered, leaving the service
#      without a resolvable code preimage ("Service code not found").
#   3. This repo built: cargo build --release -p polkadot-omni-node -p parachain-template-runtime
#
# The demo parachain uses PARA ID 0: under the dev-genesis null authorizer (empty
# config) parasim falls back to ParaId(0), so the collator must build, submit and
# watch para 0 for the loop to close.
#
# Usage: JAM_RPC=ws://127.0.0.1:19800 JAM_SERVICE_ID=5 cumulus/scripts/jam-collator-demo.sh

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
JAM_RPC="${JAM_RPC:-ws://127.0.0.1:19800}"
JAM_SERVICE_ID="${JAM_SERVICE_ID:-5}"
JAM_CORE="${JAM_CORE:-0}"
WORK_DIR="${WORK_DIR:-$(mktemp -d /tmp/jam-collator-demo.XXXXXX)}"

OMNI_NODE="$ROOT/target/release/polkadot-omni-node"
RUNTIME_WASM="$(ls "$ROOT"/target/release/wbuild/parachain-template-runtime/parachain_template_runtime.compact.compressed.wasm)"

# 1. Chain spec: template runtime, dev preset, para id 0, JAM marker.
"$OMNI_NODE" chain-spec-builder --chain-spec-path "$WORK_DIR/jam-parachain-spec.json" \
	create --relay-chain jam --para-id 0 -r "$RUNTIME_WASM" named-preset development

python3 - "$WORK_DIR/jam-parachain-spec.json" <<'EOF'
import json, sys
path = sys.argv[1]
spec = json.load(open(path))
# The preset pins para id 1000; the JAM demo needs para 0 (parasim's null-authorizer fallback).
patch = spec["genesis"]["runtimeGenesis"]["patch"]
patch.setdefault("parachainInfo", {})["parachainId"] = 0
spec["para_id"] = 0
spec["relay_chain"] = "jam"
json.dump(spec, open(path, "w"), indent=2)
EOF
echo "chain spec: $WORK_DIR/jam-parachain-spec.json"

# 2. The collator. Watch the logs (target 'jam-collator' and 'jam-rpc-interface'):
#    JAM best/finalized blocks tick, blocks get built, work packages submitted,
#    status reaches Reported, and the para head advances in JAM state.
exec "$OMNI_NODE" \
	--chain "$WORK_DIR/jam-parachain-spec.json" \
	--collator --alice \
	--jam-rpc-urls "$JAM_RPC" \
	--jam-service-id "$JAM_SERVICE_ID" \
	--jam-core "$JAM_CORE" \
	--base-path "$WORK_DIR/collator" \
	--force-authoring \
	-l jam-collator=debug,jam-rpc-interface=debug
