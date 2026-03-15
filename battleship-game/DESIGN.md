# Battleship Game — Design Decisions

## Architecture Overview

```
Relay Chain (Rococo Local)
       │
Parachain (ID 2000) ─── Battleship Pallet + Statement Store
       │
   P2P Network
   ┌───┴───┐
Smoldot    Smoldot
(Browser)  (Node.js Bot)
```

Both the browser UI and the bot connect as **smoldot light clients** — no RPC
server is needed. They join the relay chain's P2P network, then the parachain's
P2P network, and interact with the on-chain pallet and the statement store
gossip layer directly.

## Key Design Decisions

### 1. Statement Store for Game Discovery (not on-chain)

Games are **announced off-chain** via the statement store gossip protocol.
Only when an opponent actually wants to join does the creator submit an on-chain
`create_game` transaction. This avoids wasting gas on unclaimed games.

**Flow:**
1. Creator broadcasts `GameAnnouncement` on `GAME_LOBBY_TOPIC`
2. Prospective opponent sends a `LivenessPing` to verify creator is online
3. Creator responds with `LivenessPong`
4. Opponent sends `JoinRequest`
5. Creator creates the game **on-chain** and sends `GameCreatedNotification`
   with the on-chain game ID
6. Opponent joins the on-chain game

**Consequence:** Both players **must** use smoldot to participate in the
statement store gossip. A direct WebSocket RPC connection to a node cannot
subscribe to statement store topics. This means the `?smoldotProxy=` mode
in the browser UI **cannot discover or announce games** — it only works for
on-chain interactions.

### 2. Smoldot Light Client (not RPC)

The game is designed to work without any trusted RPC endpoint. Both the browser
and the bot embed smoldot and connect directly to the P2P network.

**Trade-offs:**
- Initial sync takes 10–60s depending on network age
- The chain spec must include a `lightSyncState` checkpoint that is past the
  first two BABE epochs (≥200 blocks on Rococo Local), otherwise smoldot
  rejects it with `GenesisBlockCheckpoint`
- The `runtime-download-error: StorageQuery(StorageQueryError { errors: [] })`
  can occur when smoldot can't fetch the runtime WASM from peers — this is
  transient and resolves as more peers connect
- Relay chain nodes should ideally use `--state-pruning=archive` to ensure
  smoldot can always download the runtime from any finalized block

### 3. Merkle Tree Grid Commitment

Each player's 10×10 grid (100 cells) is committed as a blake2b Merkle root.
Cells contain a random 32-byte salt + occupied flag. During battle, the
defender reveals individual cells with Merkle proofs.

This ensures:
- Ships are hidden until revealed
- Revealed cells are cryptographically bound to the original commitment
- The winner must reveal their full grid for validation (ship placement rules)

### 4. On-Chain Game State Machine

```
WaitingForOpponent → Setup → Playing → PendingWinnerReveal → Finished
                                   └→ Surrender → Finished
```

- **WaitingForOpponent:** Creator has staked funds, waiting for join
- **Setup:** Both players commit their grid roots
- **Playing:** Turn-based attack/reveal cycle
- **PendingWinnerReveal:** All 17 ship cells hit; winner must reveal full grid
- **Finished:** Funds distributed

### 5. Nonce Management

Smoldot's view of the chain can lag behind. The bot uses **nonce hedging**:
it submits transactions with both nonce N and N+1 to handle the case where
smoldot hasn't seen the latest finalized block. Per-address nonce caches
are reset on fork detection.

### 6. Fork Detection

The bot tracks `lastAttackRound` per game. If the on-chain round is less than
the last known round, a reorg occurred and the bot resets its local state for
that game.

### 7. Chain Spec Generation

`ui/generate-chain-specs.sh` reads from a running zombienet:
1. Fetches `sync_state_genSyncSpec` from relay RPC → `lightSyncState`
2. Replaces raw genesis with `stateRootHash` (smaller spec)
3. Injects boot node multiaddrs from `zombie.json`
4. Writes to both `ui/src/chain/chainSpecs.ts` and `bot/src/*.json`

**Important:** The script must be run **after** the relay chain has finalized
past at least 2 BABE epochs. With epoch duration of 100 slots and ~6s block
time, this means waiting ~20 minutes after zombienet start.

## Network Setup

Defined in `network/battleship.toml`:
- **Relay:** 2 validators (alice, bob) with `--network-backend=libp2p`
- **Parachain:** 3 collators (charlie, dave, eve) with:
  - `--enable-statement-store` (required for gossip)
  - `--state-pruning=archive` (required for smoldot runtime download)
  - `--ipfs-server` (statement store storage)
  - `--authoring=slot-based`

## Ship Definitions

| Ship         | Size |
|-------------|------|
| Carrier     | 5    |
| Battleship  | 4    |
| Cruiser     | 3    |
| Submarine   | 3    |
| Destroyer   | 2    |

Total occupied cells: 17. Game ends when all 17 are hit.

## Testing

### Bot Unit Test (`bot/tests/bot-game.test.ts`)
Full game simulation between two PAPI clients. Tests the complete
create → join → commit → attack → reveal cycle.

### Browser Integration Test (`ui/tests/bot-vs-browser.spec.ts`)
Playwright test that starts the bot as a child process and opens the UI in
Chromium. The browser joins the bot's game and plays to completion.

**Requirements:**
- Running zombienet with `battleship.toml`
- Chain specs generated after parachain produces blocks
- System Chromium on NixOS (set `PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH`)
