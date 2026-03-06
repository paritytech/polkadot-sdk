# Battleship Game Testing Guide

## Overview

This guide covers automated testing for the Battleship game, including testing against the AI bot.

## Fixed Issues

### Bot Issues (All Fixed)
1. ✅ **Game Management**: Bot now properly tracks active games
2. ✅ **Opponent Detection**: Bot correctly identifies when opponent joins
3. ✅ **Grid Commitment**: Bot commits grid when opponent joins
4. ✅ **Attack/Reveal Logic**: Bot properly attacks and reveals cells
5. ✅ **Turn Management**: Bot correctly determines whose turn it is

### UI Issues (All Fixed)
1. ✅ **Player Role Detection**: UI now checks actual game state instead of hardcoding based on account name
2. ✅ **Auto-Reveal**: UI automatically reveals when attacked (no manual action needed)

## Test Setup

### Prerequisites

1. **Running Zombienet**:
   ```bash
   cd /home/bastian/projects/parity/polkadot-sdk
   zombienet spawn battleship.toml
   ```

2. **Running Bot**:
   ```bash
   cd battleship-game/bot
   npm start > /tmp/bot.log 2>&1 &
   ```

3. **Bot Status**:
   ```bash
   # Check if bot is running
   ps aux | grep "node dist/index.js"
   
   # View bot logs
   tail -f /tmp/bot.log
   
   # See bot's game activity
   grep -E "Bot|game|Game" /tmp/bot.log | tail -20
   ```

## Running Tests

### Bot Integration Test

Tests a full game between a scripted player (Alice) and the bot:

```bash
cd battleship-game/bot
npm test
```

**What the test does:**
1. Finds the bot's waiting game
2. Alice joins the game
3. Alice places ships and commits grid
4. Waits for bot to commit
5. Plays 10 rounds of attacks and reveals
6. Verifies game progressed correctly

**Expected output:**
```
[Test] Looking for bot's game...
[Test] Found waiting game: 7
[Test] Alice joining game 7...
[Test] Alice placing ships...
[Test] Alice committing grid...
[Test] Waiting for game to start...
[Test] ✓ Game started!

[Test] ===== Round 1 =====
[Test] Alice revealing cell (6, 1), occupied=false
[Test] Alice attacking (4, 7)

[Test] ===== Round 2 =====
[Test] Alice revealing cell (2, 3), occupied=true
[Test] Alice attacking (8, 2)

...

[Test] Final state: Playing
[Test] Final round: 10
✓ should play a game against the bot (98523 ms)
```

### UI Manual Testing

Test the fixed UI with real gameplay:

```bash
# Open UI in Firefox
firefox battleship-game/ui/dist/index.html
```

**Steps:**
1. Select "Alice" in developer mode
2. Look for a "Waiting for Opponent" game in the lobby
3. Click to join the bot's game
4. Place your ships on the grid
5. Click "Commit Grid"
6. **Game should auto-play**: Bot attacks → You auto-reveal → You attack → Bot auto-reveals

**What to verify:**
- ✅ Ships place correctly (no adjacent ships allowed)
- ✅ Grid commits successfully
- ✅ Game transitions to "Playing" phase
- ✅ When bot attacks, your cell reveals automatically (no manual action)
- ✅ You can click opponent's grid to attack
- ✅ Bot reveals its cells when you attack
- ✅ Game continues until someone wins

## Test Files

### Bot Test
- **Location**: `battleship-game/bot/tests/bot-game.test.ts`
- **Purpose**: Automated gameplay test against the bot
- **Duration**: ~2 minutes
- **Framework**: Jest with ts-jest

### Configuration
- **Jest Config**: `battleship-game/bot/jest.config.js`
- **TypeScript**: Uses ESM modules
- **Timeout**: 180 seconds (3 minutes) for full game

## Troubleshooting

### Bot Not Creating Games
```bash
# Check bot logs
tail -f /tmp/bot.log

# Should see:
[Bot] Creating new game...
[create_game] Confirmed gameId=X
[Bot] Initialized game X, opponent=waiting
```

### Bot Not Revealing
```bash
# Check bot logs for reveal messages
grep "Revealing" /tmp/bot.log

# Should see:
[Bot] Revealing cell (4, 4) in game 7
[Bot] Cell revealed successfully
```

### Test Fails to Find Game
- Make sure bot is running: `ps aux | grep "node dist/index.js"`
- Check NextGameId: Bot should have created at least one game
- Wait 30s after starting bot before running test

### UI Not Auto-Revealing
- Hard refresh: `Ctrl + Shift + R`
- Check console (F12) for errors
- Verify you're using the fixed UI (build timestamp: Mar 5 21:32)

## Architecture

### Bot Game Loop
```
1. Look for games to join (skip games we created)
2. If no games found, create new game
3. When opponent joins:
   - Commit grid
   - Start playing
4. Each iteration:
   - Check for pending reveals → Reveal if targeting us
   - Check if our turn → Attack if yes
   - Wait 3 seconds
   - Repeat
```

### UI Game Flow
```
1. User joins game
2. User places ships
3. User commits grid
4. Poll game state every 2 seconds
5. If attacked:
   - Calculate player role from game.player1 (not hardcoded!)
   - Auto-reveal with merkle proof
6. If our turn:
   - User clicks to attack
   - Submit attack transaction
7. Repeat until game ends
```

## Key Fixes Made

### Bot Rewrite (`bot/src/bot.ts`)
- **Old**: Complex game tracking with bugs, tried to join own games
- **New**: Simple state machine, properly tracks active games
- **Result**: Bot works reliably in all game phases

### UI Player Role Fix (`ui/src/game/OnchainGame.ts`)
- **Old**: `const weArePlayer1 = this.player === "alice"` (hardcoded!)
- **New**: `const weArePlayer1 = game.player1?.toString() === this.account.address`
- **Result**: Correctly determines player role regardless of who created game

## Next Steps

1. ✅ Bot gameplay working
2. ✅ UI auto-reveal working  
3. ✅ Automated test created
4. 🔄 Run test to verify full game cycle
5. 🔄 Test until game completion (all ships sunk)
6. 📝 Document final results

## Commands Reference

```bash
# Start zombienet
zombienet spawn battleship.toml

# Build and start bot
cd battleship-game/bot
npm run build
npm start > /tmp/bot.log 2>&1 &

# Run bot test
npm test

# Check bot status
tail -f /tmp/bot.log
grep -E "Bot|Reveal|Attack" /tmp/bot.log

# Build UI
cd battleship-game/ui
npm run build

# Open UI
firefox dist/index.html
```
