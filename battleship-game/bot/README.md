# Battleship Bot

An automated bot that plays the Battleship game on the blockchain using a smoldot light client.

## Features

- **Light Client**: Uses smoldot to connect directly to the network without needing a full node
- Automatically creates games or joins existing waiting games
- Places ships randomly on the grid with proper validation
- Uses AI strategy for attacking:
  - **Hunt mode**: When hits are detected, targets adjacent cells
  - **Search mode**: Random targeting when no hits exist
- Automatically reveals cells when attacked
- Handles multiple games simultaneously

## Setup

1. Install dependencies:
```bash
npm install
```

2. Build the TypeScript code:
```bash
npm run build
```

## Running

Start the bot with:
```bash
npm start
```

The bot generates a random account on each startup and requests funds from the on-chain faucet.

## How It Works

1. **Light Client Initialization**: The bot starts a smoldot light client that connects to the relay chain and parachain
2. **Game Discovery**: The bot continuously scans for waiting games
3. **Join or Create**: 
   - If a waiting game exists, the bot joins it
   - If no waiting games exist, the bot creates a new game
4. **Ship Placement**: Random ship placement with validation (no adjacent ships)
5. **Grid Commitment**: Submits merkle root of the grid to the chain
6. **Game Loop**:
   - On bot's turn: Selects target using AI strategy and attacks
   - When attacked: Reveals the cell with merkle proof
   - Continues until game ends

## AI Strategy

The bot uses a two-mode attack strategy:

- **Search Mode**: Random cell selection when no hits exist
- **Hunt Mode**: When a hit is detected, targets all adjacent cells (north, south, east, west) to sink the ship

This is a simple but effective strategy that mimics human gameplay.

## Architecture

### Smoldot Light Client

The bot uses smoldot, a light client implementation that:
- Doesn't require a full node connection
- Connects directly to the p2p network
- Verifies all data cryptographically
- Uses minimal resources compared to a full node

### Chain Specs

The bot connects to:
- **Relay Chain**: Rococo local testnet
- **Parachain**: Battleship parachain (paraId: 2000)

Chain specifications are embedded in `chainSpecs.ts`.

## Files

- `src/accounts.ts` - Random account generation and faucet funding
- `src/client.ts` - Smoldot light client initialization
- `src/chainSpecs.ts` - Relay chain and parachain specifications
- `src/battleship.ts` - Wrapper for battleship pallet calls
- `src/game.ts` - Ship placement and AI attack strategy
- `src/merkle.ts` - Merkle tree implementation for commitments
- `src/types.ts` - Type definitions and constants
- `src/bot.ts` - Main bot logic and game loop
- `src/index.ts` - Entry point

## Testing

To test the bot, you need:

1. Running relay chain and parachain nodes with the battleship pallet
2. Nodes configured with the boot nodes specified in `chainSpecs.ts`
3. Another player (human or another bot instance) to play against

The bot will automatically discover the network through the boot nodes specified in the chain specs.

## Smoldot Version

This bot uses the local smoldot checkout from `/home/bastian/projects/parity/smoldot`, which is linked in `package.json`.
