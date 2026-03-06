# Battleship with Smoldot Light Clients

This document describes the complete setup of the Battleship game using smoldot light clients for both the UI and the bot.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     Blockchain Network                        │
│                                                               │
│  ┌──────────────┐              ┌──────────────┐            │
│  │ Relay Chain  │──────────────│  Parachain   │            │
│  │   (Rococo)   │              │ (Battleship) │            │
│  └──────────────┘              └──────────────┘            │
│         │                              │                     │
│         │                              │                     │
└─────────┼──────────────────────────────┼─────────────────────┘
          │                              │
          │         P2P Network          │
          │                              │
    ┌─────┴─────────────────────────────┴─────┐
    │                                           │
    │                                           │
┌───▼────────────┐                    ┌────────▼────┐
│ Smoldot (UI)   │                    │ Smoldot (Bot)│
│ Light Client   │                    │ Light Client │
│ (Browser)      │                    │ (Node.js)    │
└────────────────┘                    └──────────────┘
         │                                    │
         │                                    │
    ┌────▼─────┐                        ┌────▼─────┐
    │ Web UI   │                        │   Bot    │
    │ (Player) │                        │ (Player) │
    └──────────┘                        └──────────┘
```

## Components

### 1. Blockchain Network

**Location**: `/home/bastian/projects/parity/polkadot-sdk/battleship-game/`

#### Relay Chain (Rococo Local)
- Local Rococo testnet
- Boot node: `/ip4/127.0.0.1/tcp/35439/ws/p2p/12D3KooWQCkBm1BYtkHpocxCwMgR8yjitEeHGx8spzcDLGt2gkBm`

#### Parachain (Battleship)
- ParaId: 2000
- Contains battleship pallet
- Boot node: `/ip4/127.0.0.1/tcp/44071/ws/p2p/12D3KooWPKzmmE2uYgF3z13xjpbFTp63g9dZFag8pG6MgnpSLF4S`

### 2. Web UI (Browser Light Client)

**Location**: `battleship-game/ui/`

**Built file**: `ui/dist/index.html` (31MB single file)

**Features**:
- Self-contained HTML with embedded smoldot WASM
- No server required (runs entirely in browser)
- Developer mode (Alice/Bob) for local testing
- Wallet extension support for production
- Real-time game UI with ship placement and attacks

**Run**:
```bash
cd battleship-game/ui
./serve.sh
# Open http://localhost:8080
```

**Smoldot version**: 2.0.40 (from npm)

### 3. Bot (Node.js Light Client)

**Location**: `battleship-game/bot/`

**Features**:
- Autonomous player that creates/joins games
- AI strategy: hunt mode (when hits exist) + search mode (random)
- Automatic ship placement with validation
- Handles multiple games simultaneously

**Run**:
```bash
cd battleship-game/bot
npm start
```

**Smoldot version**: `/home/bastian/projects/parity/smoldot` (local checkout)

**Account**: Charlie (`//Charlie` dev account)

## Smoldot Integration

Both the UI and bot use the same smoldot architecture:

### Initialization Flow

1. **Start smoldot client**:
   ```typescript
   const smoldot = start({ maxLogLevel: 4, logCallback: ... });
   ```

2. **Add relay chain**:
   ```typescript
   const relayChain = await smoldot.addChain({ chainSpec: relayChainSpec });
   ```

3. **Add parachain** (linked to relay):
   ```typescript
   const parachain = await smoldot.addChain({
     chainSpec: parachainSpec,
     potentialRelayChains: [relayChain],
   });
   ```

4. **Create polkadot-api client**:
   ```typescript
   const client = createClient(getSmProvider(() => parachain));
   ```

### Chain Specifications

Both UI and bot use identical chain specs from `chainSpecs.ts`:
- Relay chain spec (Rococo local with genesis state)
- Parachain spec (Battleship with genesis state)
- Boot nodes for p2p network discovery

## Complete Setup Guide

### Prerequisites

1. **Running blockchain network**:
   ```bash
   # Start relay chain
   polkadot --chain rococo-local --alice --tmp \
     --listen-addr=/ip4/127.0.0.1/tcp/35439/ws

   # Start parachain
   polkadot-parachain --chain battleship-local --collator \
     --alice --tmp --listen-addr=/ip4/127.0.0.1/tcp/44071/ws
   ```

2. **Extract chain specs** (if needed):
   ```bash
   cd battleship-game/ui
   ./extract-chain-specs.sh
   # Copy chainSpecs.ts to bot if changed
   ```

### Running the Complete Stack

#### Terminal 1: Start UI
```bash
cd battleship-game/ui
./serve.sh
```

#### Terminal 2: Start Bot
```bash
cd battleship-game/bot
npm start
```

#### Terminal 3: Play the Game
```bash
# Open browser
firefox http://localhost:8080

# Or Chrome
google-chrome http://localhost:8080
```

**In the browser**:
1. Enable "Developer Mode"
2. Select "Alice"
3. Click "Continue to Lobby"
4. Wait for bot to create a game OR create your own

**The bot will**:
1. Connect via smoldot (takes ~10-30s)
2. Either join your game OR create a new one
3. Automatically place ships and start playing

## Testing Scenarios

### Scenario 1: UI vs Bot

1. **UI**: Alice creates a game with 1 UNIT stake
2. **Bot**: Detects the waiting game and joins
3. Both commit grids
4. Take turns attacking until winner

### Scenario 2: Bot vs Bot

1. Modify `bot/src/accounts.ts` to use different accounts
2. Run two bot instances in separate terminals
3. Bots will play against each other

### Scenario 3: UI vs UI (Two Players)

1. Open two browser tabs
2. **Tab 1**: Alice
3. **Tab 2**: Bob
4. Alice creates game, Bob joins

## Benefits of Smoldot

| Aspect | Traditional (RPC) | Smoldot Light Client |
|--------|------------------|---------------------|
| **Infrastructure** | Requires full node with RPC | Self-contained, p2p network |
| **Security** | Trusts RPC endpoint | Byzantine-resilient, verifies everything |
| **Portability** | Needs hosted infrastructure | Runs anywhere (browser, Node.js) |
| **Censorship** | RPC can censor | Direct p2p, uncensorable |
| **Setup** | Complex node deployment | Just embed in app |
| **Cost** | RPC hosting costs | Free (users run light client) |

## Smoldot Versions

### UI: NPM Version
```json
"smoldot": "^2.0.40"
```
- Downloaded from npm
- Embedded in single HTML file
- ~30MB of the 31MB total size

### Bot: Local Checkout
```bash
/home/bastian/projects/parity/smoldot/wasm-node/javascript
```
- Local development version
- Installed via: `npm install /home/bastian/projects/parity/smoldot/wasm-node/javascript`
- Allows testing smoldot changes immediately

## Network Discovery

Smoldot discovers the network through boot nodes:

1. **Connect to boot nodes** (hardcoded in chain specs)
2. **Request peer list** from boot nodes
3. **Connect to peers** in the network
4. **Sync chain state** from peers
5. **Ready to submit transactions**

This takes ~10-30 seconds on first connection.

## Troubleshooting

### UI not connecting
- Check browser console for smoldot logs
- Verify relay/parachain nodes are running
- Ensure boot nodes match running nodes

### Bot not connecting
- Check bot logs for smoldot errors
- Verify Node.js version (need 18+)
- Ensure smoldot WASM can run in Node.js

### Games not appearing
- Both clients must connect to the same chain
- Wait for smoldot to sync (10-30s)
- Check that NextGameId is incrementing on-chain

### Slow performance
- Smoldot needs time for initial sync
- First game creation may take 30-60s
- Subsequent actions are faster

## File References

### Chain Specs
- `ui/src/chain/chainSpecs.ts` - Web UI chain specs
- `bot/src/chainSpecs.ts` - Bot chain specs (copy of UI's)

### Smoldot Initialization
- `ui/src/chain/client.ts` - Browser smoldot setup
- `bot/src/client.ts` - Node.js smoldot setup

### Game Logic
- `ui/src/game/OnchainGame.ts` - UI game manager
- `bot/src/bot.ts` - Bot game loop

### Blockchain Interaction
- `ui/src/chain/battleship.ts` - UI pallet wrapper
- `bot/src/battleship.ts` - Bot pallet wrapper

## Summary

✅ **UI**: Browser-based light client using smoldot
✅ **Bot**: Node.js light client using smoldot (local checkout)
✅ **Network**: Relay + parachain with battleship pallet
✅ **No RPC needed**: Direct p2p connections via smoldot
✅ **Fully decentralized**: No trusted intermediaries
✅ **Ready to play**: Just start nodes, UI, and bot!

The entire stack demonstrates how to build decentralized applications that don't rely on centralized RPC servers. Users run light clients that connect directly to the blockchain network.
