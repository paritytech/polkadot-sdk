#!/usr/bin/env bash
set -e

# Generate optimized chain specs for smoldot from a running zombienet network.
# Usage: ./generate-chain-specs.sh <relay_rpc_port> <relay_listen_port> <parachain_rpc_port> <parachain_listen_port>

RELAY_RPC=${1:?Usage: $0 <relay_rpc_port> <relay_listen_port> <parachain_rpc_port> <parachain_listen_port>}
RELAY_LISTEN=${2:?}
PARA_RPC=${3:?}
PARA_LISTEN=${4:?}

SCRIPT_DIR="$(dirname "$0")"
UI_DIR="$SCRIPT_DIR/../ui"
CHAIN_SPECS_FILE="$UI_DIR/src/chain/chainSpecs.ts"

echo "Fetching light sync spec from relay chain (port $RELAY_RPC)..."

python3 << PYEOF
import json, urllib.request, sys

def rpc(url, method, params=[]):
    data = json.dumps({"id":1, "jsonrpc":"2.0", "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        result = json.loads(resp.read())
        if "error" in result:
            print(f"RPC error: {result['error']}", file=sys.stderr)
            sys.exit(1)
        return result["result"]

relay_url = "http://127.0.0.1:${RELAY_RPC}"
para_url = "http://127.0.0.1:${PARA_RPC}"

# Get relay chain light sync spec (includes lightSyncState)
relay_spec = rpc(relay_url, "sync_state_genSyncSpec", [True])

# Get genesis state roots
relay_genesis_hash = rpc(relay_url, "chain_getBlockHash", [0])
relay_header = rpc(relay_url, "chain_getHeader", [relay_genesis_hash])
relay_state_root = relay_header["stateRoot"]

para_genesis_hash = rpc(para_url, "chain_getBlockHash", [0])
para_header = rpc(para_url, "chain_getHeader", [para_genesis_hash])
para_state_root = para_header["stateRoot"]

# Get peer IDs
relay_peer_id = rpc(relay_url, "system_localPeerId")
para_peer_id = rpc(para_url, "system_localPeerId")

# Build optimized relay spec
relay_opt = {
    "bootNodes": [f"/ip4/127.0.0.1/tcp/${RELAY_LISTEN}/ws/p2p/{relay_peer_id}"],
    "chainType": relay_spec["chainType"],
    "codeSubstitutes": {},
    "genesis": {"stateRootHash": relay_state_root},
    "id": relay_spec["id"],
    "lightSyncState": relay_spec["lightSyncState"],
    "name": relay_spec["name"],
    "properties": relay_spec.get("properties"),
    "protocolId": relay_spec.get("protocolId"),
    "telemetryEndpoints": relay_spec.get("telemetryEndpoints")
}

# Build optimized parachain spec
para_opt = {
    "bootNodes": [f"/ip4/127.0.0.1/tcp/${PARA_LISTEN}/ws/p2p/{para_peer_id}"],
    "chainType": "Local",
    "codeSubstitutes": {},
    "genesis": {"stateRootHash": para_state_root},
    "id": "",
    "name": "",
    "para_id": 2000,
    "properties": None,
    "protocolId": None,
    "relay_chain": relay_spec["id"],
    "telemetryEndpoints": None
}

relay_json = json.dumps(relay_opt, indent=2)
para_json = json.dumps(para_opt, indent=2)

ts = f'''export const relayChainSpec = \x60
{relay_json}\x60;

export const parachainSpec = \x60
{para_json}\x60;
'''

with open("${CHAIN_SPECS_FILE}", "w") as f:
    f.write(ts)

print(f"Generated {len(relay_json)+len(para_json)} bytes chain specs to ${CHAIN_SPECS_FILE}")
PYEOF
