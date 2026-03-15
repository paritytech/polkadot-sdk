#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(dirname "$0")"
cd "$SCRIPT_DIR"

CHAIN_SPECS_DIR="public/chain-specs"
OUTPUT_FILE="src/chain/chainSpecs.ts"

if [ ! -f "$CHAIN_SPECS_DIR/relay.json" ] || [ ! -f "$CHAIN_SPECS_DIR/parachain.json" ]; then
    echo "Error: Chain specs not found. Run ./extract-chain-specs.sh first"
    exit 1
fi

echo "Generating optimized chain specs for smoldot..."

python3 << 'PYEOF'
import json, urllib.request, sys, os, glob

CHAIN_SPECS_DIR = "public/chain-specs"
OUTPUT_FILE = "src/chain/chainSpecs.ts"

def rpc(url, method, params=[]):
    data = json.dumps({"id":1, "jsonrpc":"2.0", "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            result = json.loads(resp.read())
            if "error" in result:
                return None
            return result["result"]
    except Exception:
        return None

def get_genesis_state_root(rpc_url):
    genesis_hash = rpc(rpc_url, "chain_getBlockHash", [0])
    if not genesis_hash:
        return None
    header = rpc(rpc_url, "chain_getHeader", [genesis_hash])
    if not header:
        return None
    return header["stateRoot"]

def find_zombie_dir():
    """Find the most recent zombienet temp directory."""
    dirs = sorted(glob.glob("/tmp/zombie-*"), key=os.path.getmtime, reverse=True)
    for d in dirs:
        if os.path.isfile(os.path.join(d, "zombie.json")):
            return d
    return None

def get_node_info_from_zombie(zombie_dir):
    """Read zombie.json to get relay and parachain node RPC URLs and boot node multiaddrs."""
    with open(os.path.join(zombie_dir, "zombie.json")) as f:
        state = json.load(f)

    relay_url = None
    para_url = None
    relay_bootnodes = []
    para_bootnodes = []

    # Get relay chain node URLs and multiaddrs
    relay_nodes = state.get("relay", {}).get("nodes", [])
    for node in relay_nodes:
        ws_uri = node.get("ws_uri", "")
        multiaddr = node.get("multiaddr", "")
        if ws_uri and not relay_url:
            relay_url = ws_uri.replace("ws://", "http://").replace("wss://", "https://")
        if multiaddr:
            relay_bootnodes.append(multiaddr)

    # Get parachain collator URLs and multiaddrs
    for para_id, para_list in state.get("parachains", {}).items():
        for para in para_list:
            for collator in para.get("collators", []):
                ws_uri = collator.get("ws_uri", "")
                multiaddr = collator.get("multiaddr", "")
                if ws_uri and not para_url:
                    para_url = ws_uri.replace("ws://", "http://").replace("wss://", "https://")
                if multiaddr:
                    para_bootnodes.append(multiaddr)
        if para_url:
            break

    return relay_url, para_url, relay_bootnodes, para_bootnodes

# Find zombienet and get node URLs
zombie_dir = find_zombie_dir()
if not zombie_dir:
    print("Error: No zombienet directory with zombie.json found in /tmp", file=sys.stderr)
    print("Make sure zombienet is running", file=sys.stderr)
    sys.exit(1)

print(f"Using zombienet at: {zombie_dir}")
relay_url, para_url, relay_bootnodes, para_bootnodes = get_node_info_from_zombie(zombie_dir)

if not relay_url:
    print("Error: Could not find relay chain node URL in zombie.json", file=sys.stderr)
    sys.exit(1)
if not para_url:
    print("Error: Could not find parachain node URL in zombie.json", file=sys.stderr)
    sys.exit(1)

print(f"Relay chain RPC: {relay_url}")
print(f"Parachain RPC:   {para_url}")
print(f"Relay boot nodes: {relay_bootnodes}")
print(f"Para boot nodes:  {para_bootnodes}")

# Fetch light sync state and genesis state roots
relay_sync_spec = rpc(relay_url, "sync_state_genSyncSpec", [True])
if not relay_sync_spec:
    print("Error: Could not get relay chain sync spec", file=sys.stderr)
    sys.exit(1)

relay_state_root = get_genesis_state_root(relay_url)
if not relay_state_root:
    print("Error: Could not get relay chain genesis state root", file=sys.stderr)
    sys.exit(1)
print(f"Relay genesis state root: {relay_state_root}")

para_state_root = get_genesis_state_root(para_url)
if not para_state_root:
    print("Error: Could not get parachain genesis state root", file=sys.stderr)
    sys.exit(1)
print(f"Parachain genesis state root: {para_state_root}")

# Read chain spec JSON files and add lightSyncState + bootNodes
# Keep the raw genesis so smoldot can extract the runtime locally
# (the light client storage query protocol often fails on local testnets)
with open(f"{CHAIN_SPECS_DIR}/relay.json") as f:
    relay_spec = json.load(f)
with open(f"{CHAIN_SPECS_DIR}/parachain.json") as f:
    para_spec = json.load(f)

relay_spec["lightSyncState"] = relay_sync_spec.get("lightSyncState")
relay_spec["bootNodes"] = relay_bootnodes

para_spec["bootNodes"] = para_bootnodes

relay_json = json.dumps(relay_spec, separators=(',', ':'))
para_json = json.dumps(para_spec, separators=(',', ':'))

ts = f'export const relayChainSpec = `\n{relay_json}`;\n\nexport const parachainSpec = `\n{para_json}`;\n'

with open(OUTPUT_FILE, "w") as f:
    f.write(ts)

print(f"Generated {OUTPUT_FILE} ({len(ts)} bytes, with raw genesis + lightSyncState)")

# Also update bot chain spec files
BOT_DIR = os.path.join("..", "bot", "src")
bot_relay = os.path.join(BOT_DIR, "relay-chain-spec.json")
bot_para = os.path.join(BOT_DIR, "parachain-spec.json")

if os.path.isdir(BOT_DIR):
    with open(bot_relay, "w") as f:
        json.dump(relay_spec, f, indent=2)
        f.write("\n")
    with open(bot_para, "w") as f:
        json.dump(para_spec, f, indent=2)
        f.write("\n")
    print(f"Updated bot chain specs in {BOT_DIR}")
else:
    print(f"Warning: Bot directory {BOT_DIR} not found, skipping bot chain specs")
PYEOF
