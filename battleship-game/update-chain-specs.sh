#!/usr/bin/env bash
# Update chain specs for both UI and bot from the currently running zombienet.
# Usage: ./update-chain-specs.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MAX_RETRIES=30
RETRY_INTERVAL=5

find_latest_zombienet_dir() {
  ls -td /tmp/zombie-* 2>/dev/null | head -1 || true
}

extract_from_processes() {
  ZOMBIE_DIR=$(ps aux | grep 'polkadot.*--name alice' | grep -v grep | grep -oP '/tmp/zombie-[a-f0-9-]+' | head -1 || true)
  RELAY_RPC_PORT=$(ps aux | grep 'polkadot.*--name alice' | grep -v grep | grep -oP -- '--rpc-port\s+\K[0-9]+' | head -1 || true)
  RELAY_PORT=$(ps aux | grep 'polkadot.*--name alice' | grep -v grep | grep -oP 'listen-addr /ip4/0.0.0.0/tcp/\K[0-9]+' | head -1 || true)
  PARA_RPC_PORT=$(ps aux | grep 'polkadot.*--name charlie' | grep -v grep | grep -oP -- '--rpc-port\s+\K[0-9]+' | head -1 || true)
  PARA_PORT=$(ps aux | grep 'polkadot.*--name charlie' | grep -v grep | grep -oP 'listen-addr /ip4/0.0.0.0/tcp/\K[0-9]+' | head -1 || true)
}

extract_from_zombie_json() {
  local zombie_json=$1
  mapfile -t zombie_fields < <(
    python3 - "$zombie_json" <<'PY'
import json
import sys

with open(sys.argv[1]) as f:
    data = json.load(f)

relay_nodes = data["relay"]["nodes"]
para_entries = data["parachains"]["2000"]
if isinstance(para_entries, list):
    para_nodes = []
    for entry in para_entries:
        para_nodes.extend(entry.get("collators", []))
else:
    para_nodes = para_entries["nodes"]

relay = next(node for node in relay_nodes if node["name"] == "alice")
para = next(node for node in para_nodes if node["name"] == "charlie")

def print_field(value):
    if isinstance(value, list):
        print(value[0] if value else "")
    else:
        print(value or "")

def arg_value(node, flag):
    args = node["inner"]["args"]
    if flag in args:
        idx = args.index(flag)
        if idx + 1 < len(args):
            return args[idx + 1]
    return ""

print(data["local_base_dir"])
print(arg_value(relay, "--rpc-port"))
print_field(relay.get("multiaddr"))
print(arg_value(para, "--rpc-port") or para.get("ws_uri", "").rsplit(":", 1)[-1])
print_field(para.get("multiaddr"))
PY
  )

  ZOMBIE_DIR="${zombie_fields[0]:-}"
  RELAY_RPC_PORT="${zombie_fields[1]:-}"
  RELAY_PORT=$(printf '%s\n' "${zombie_fields[2]:-}" | grep -oP '/tcp/\K[0-9]+' | head -1 || true)
  PARA_RPC_PORT="${zombie_fields[3]:-}"
  PARA_PORT=$(printf '%s\n' "${zombie_fields[4]:-}" | grep -oP '/tcp/\K[0-9]+' | head -1 || true)
}

fetch_peer_ids() {
  mapfile -t peer_ids < <(
    python3 - "$RELAY_RPC_PORT" "$PARA_RPC_PORT" <<'PY'
import json
import sys
import urllib.request

def rpc(port):
    payload = json.dumps({
        "id": 1,
        "jsonrpc": "2.0",
        "method": "system_localPeerId",
        "params": [],
    }).encode()
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}",
        data=payload,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        data = json.loads(resp.read())
    print(data["result"])

rpc(sys.argv[1])
rpc(sys.argv[2])
PY
  )

  RELAY_PEER="${peer_ids[0]:-}"
  PARA_PEER="${peer_ids[1]:-}"
}

# Wait for the running zombienet
echo "Waiting for zombienet to be ready..."
for i in $(seq 1 $MAX_RETRIES); do
  extract_from_processes

  if [ -n "${ZOMBIE_DIR:-}" ] && [ -n "${RELAY_RPC_PORT:-}" ] && [ -n "${RELAY_PORT:-}" ] && [ -n "${PARA_RPC_PORT:-}" ] && [ -n "${PARA_PORT:-}" ]; then
    break
  fi

  ZOMBIE_DIR=$(find_latest_zombienet_dir)
  if [ -n "$ZOMBIE_DIR" ] && [ -f "$ZOMBIE_DIR/zombie.json" ]; then
    extract_from_zombie_json "$ZOMBIE_DIR/zombie.json"
    if [ -n "${ZOMBIE_DIR:-}" ] && [ -n "${RELAY_RPC_PORT:-}" ] && [ -n "${RELAY_PORT:-}" ] && [ -n "${PARA_RPC_PORT:-}" ] && [ -n "${PARA_PORT:-}" ]; then
      break
    fi
  fi

  echo "  Attempt $i/$MAX_RETRIES - zombienet not ready yet, retrying in ${RETRY_INTERVAL}s..."
  sleep $RETRY_INTERVAL
done

if [ -z "${ZOMBIE_DIR:-}" ]; then
  echo "ERROR: No running zombienet found after ${MAX_RETRIES} attempts."
  exit 1
fi

if [ -z "${RELAY_RPC_PORT:-}" ] || [ -z "${RELAY_PORT:-}" ] || [ -z "${PARA_RPC_PORT:-}" ] || [ -z "${PARA_PORT:-}" ]; then
  echo "ERROR: Could not extract RPC and P2P ports from running zombienet metadata."
  exit 1
fi

echo "Found zombienet: $ZOMBIE_DIR"

fetch_peer_ids
if [ -z "${RELAY_PEER:-}" ] || [ -z "${PARA_PEER:-}" ]; then
  echo "ERROR: Could not fetch peer IDs from the running nodes."
  exit 1
fi

RELAY_MULTIADDR="/ip4/127.0.0.1/tcp/${RELAY_PORT}/ws/p2p/${RELAY_PEER}"
PARA_MULTIADDR="/ip4/127.0.0.1/tcp/${PARA_PORT}/ws/p2p/${PARA_PEER}"

echo "Relay: $RELAY_MULTIADDR"
echo "Para:  $PARA_MULTIADDR"

# --- Update bot chain specs (JSON files) ---
BOT_RELAY="$SCRIPT_DIR/bot/src/relay-chain-spec.json"
BOT_PARA="$SCRIPT_DIR/bot/src/parachain-spec.json"

# --- Update bot + UI chain specs ---
UI_CHAINSPECS="$SCRIPT_DIR/ui/src/chain/chainSpecs.ts"

python3 -c "
import json

zombie_dir = '$ZOMBIE_DIR'
relay_multiaddr = '$RELAY_MULTIADDR'
para_multiaddr = '$PARA_MULTIADDR'
bot_relay = '$BOT_RELAY'
bot_para = '$BOT_PARA'
ui_chainspecs = '$UI_CHAINSPECS'

# Read and patch chain specs from zombienet
specs = []
for src, dst, bootnode in [
    (f'{zombie_dir}/alice/cfg/rococo-local.json', bot_relay, relay_multiaddr),
    (f'{zombie_dir}/charlie/cfg/2000.json',       bot_para,  para_multiaddr),
]:
    with open(src) as f:
        d = json.load(f)
    d['bootNodes'] = [bootnode]
    with open(dst, 'w') as f:
        json.dump(d, f)
    specs.append(d)
    print(f'  Updated {dst}')

# Generate UI chainSpecs.ts from the same data
relay_json = json.dumps(specs[0], indent=2)
para_json = json.dumps(specs[1], indent=2)

ts_content = f'export const relayChainSpec = \x60\n{relay_json}\x60;\n\nexport const parachainSpec = \x60\n{para_json}\x60;\n'

with open(ui_chainspecs, 'w') as f:
    f.write(ts_content)
print(f'  Updated {ui_chainspecs}')
"

echo ""
echo "Done! Chain specs updated for ports: relay=$RELAY_PORT, para=$PARA_PORT"
