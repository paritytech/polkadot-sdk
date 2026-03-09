#!/usr/bin/env bash
# Update chain specs for both UI and bot from the currently running zombienet.
# Usage: ./update-chain-specs.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Find the running zombienet directory
ZOMBIE_DIR=$(ps aux | grep 'polkadot.*--name alice' | grep -v grep | grep -oP '/tmp/zombie-[a-f0-9-]+' | head -1 || true)
if [ -z "$ZOMBIE_DIR" ]; then
  echo "ERROR: No running zombienet found (no polkadot alice process)."
  exit 1
fi
echo "Found zombienet: $ZOMBIE_DIR"

# Extract relay chain listen port
RELAY_PORT=$(ps aux | grep 'polkadot.*--name alice' | grep -v grep | grep -oP 'listen-addr /ip4/0.0.0.0/tcp/\K[0-9]+' | head -1)
# Extract parachain (charlie) listen port
PARA_PORT=$(ps aux | grep 'polkadot.*--name charlie' | grep -v grep | grep -oP 'listen-addr /ip4/0.0.0.0/tcp/\K[0-9]+' | head -1)

if [ -z "$RELAY_PORT" ] || [ -z "$PARA_PORT" ]; then
  echo "ERROR: Could not extract ports from running processes."
  exit 1
fi

# Fixed peer IDs (derived from well-known node keys used by zombienet)
RELAY_PEER="12D3KooWQCkBm1BYtkHpocxCwMgR8yjitEeHGx8spzcDLGt2gkBm"
PARA_PEER="12D3KooWPKzmmE2uYgF3z13xjpbFTp63g9dZFag8pG6MgnpSLF4S"

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
