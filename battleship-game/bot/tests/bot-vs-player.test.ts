import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import { BattleshipClient } from '../src/battleship.js';
import { BattleshipBot } from '../src/bot.js';
import { createRandomAccount } from '../src/accounts.js';
import { placeShipsRandomly, selectAttackTarget } from '../src/game.js';
import { buildMerkleTree } from '../src/merkle.js';
import { createNewClient, type IndependentClient } from '../src/client.js';

/**
 * Test: Bot vs Player with SEPARATE smoldot instances.
 *
 * Unlike bot-game.test.ts which shares a single smoldot client, this test
 * creates two independent light client connections — mirroring the real-world
 * scenario where a bot and a browser player each run their own smoldot.
 *
 * This exposes timing bugs where one client's "best" block lags behind the
 * other, causing stale reads of game state.
 *
 * Prerequisites:
 * - Zombienet running with battleship parachain
 */
describe('Bot vs Player (Separate Clients)', () => {
  let botConnection: IndependentClient;
  let playerConnection: IndependentClient;
  let botBattleship: BattleshipClient;
  let playerBattleship: BattleshipClient;
  let botAccount: ReturnType<typeof createRandomAccount>;
  let playerAccount: ReturnType<typeof createRandomAccount>;
  let gameId: bigint;

  beforeAll(async () => {
    botAccount = createRandomAccount();
    playerAccount = createRandomAccount();

    console.log('[Test] Creating two independent smoldot clients...');

    // Create two SEPARATE smoldot connections (the key difference from bot-game.test.ts)
    [botConnection, playerConnection] = await Promise.all([
      createNewClient('bot'),
      createNewClient('player'),
    ]);

    botBattleship = await BattleshipClient.create(botConnection.client);
    playerBattleship = await BattleshipClient.create(playerConnection.client);

    // Fund both accounts
    console.log('[Test] Requesting funds for bot and player...');
    await Promise.all([
      botBattleship.requestFunds(botAccount.address),
      playerBattleship.requestFunds(playerAccount.address),
    ]);
    await new Promise(r => setTimeout(r, 8000));
    console.log('[Test] Faucet requests submitted, proceeding...');

    // Start bot on its own client (runs in background)
    console.log('[Test] Starting bot...');
    const bot = new BattleshipBot(botBattleship, botAccount);
    bot.run().catch(e => console.error('[Test] Bot error:', e));

    // Poll from PLAYER's client for the bot's game — this crosses the smoldot boundary
    console.log('[Test] Waiting for bot to create a game (polling from player client)...');
    let found = false;
    for (let i = 0; i < 30; i++) {
      await new Promise(r => setTimeout(r, 2000));
      try {
        const waitingGames = await playerBattleship.findWaitingGames();
        for (const gId of waitingGames) {
          try {
            const game = await playerBattleship.getGame(gId);
            if (game?.player1?.toString() === botAccount.address) {
              gameId = gId;
              found = true;
              break;
            }
          } catch {}
        }
      } catch (e) {
        console.error('[Test] Error polling for games:', e);
      }
      if (found) break;
    }
    if (!found) {
      throw new Error('Bot did not create a game within timeout');
    }
    console.log(`[Test] Found bot's game ${gameId} from player's view`);
  }, 120000);

  afterAll(() => {
    console.log('[Test] Cleaning up connections...');
    try { playerConnection?.destroy(); } catch {}
    try { botConnection?.destroy(); } catch {}
  });

  it('should play a full game to completion on separate clients', async () => {
    // Player joins the game via their own client
    console.log(`[Test] Player joining game ${gameId}...`);
    const joinSuccess = await playerBattleship.joinGame(playerAccount.signer, gameId);
    expect(joinSuccess).toBe(true);

    // Player places ships and commits grid
    console.log('[Test] Player placing ships...');
    const playerGrid = placeShipsRandomly();
    const { root: playerRoot, proofs: playerProofs } = buildMerkleTree(playerGrid);

    console.log('[Test] Player committing grid...');
    const commitSuccess = await playerBattleship.commitGrid(playerAccount.signer, gameId, playerRoot);
    expect(commitSuccess).toBe(true);

    // Wait for Playing phase — bot must also commit, seen through player's client
    console.log('[Test] Waiting for Playing phase...');
    let gameStarted = false;
    for (let i = 0; i < 60; i++) {
      await new Promise(r => setTimeout(r, 1500));
      try {
        const game = await playerBattleship.getGame(gameId);
        if (game?.phase?.type === "Playing") {
          console.log('[Test] Game entered Playing phase!');
          gameStarted = true;
          break;
        }
        if (i % 10 === 0) {
          console.log(`[Test] Waiting for Playing... phase=${game?.phase?.type}`);
        }
      } catch (e) {
        console.error('[Test] Error checking phase:', e);
      }
    }
    expect(gameStarted).toBe(true);

    // Game loop — player is Player2 (joiner), bot is Player1 (creator)
    const startTime = Date.now();
    const maxDuration = 120_000;
    let lastPlayerRound = -1;
    let lastRevealCoord = "";
    const playerAttacks = new Map<string, boolean>();
    let iteration = 0;

    while (Date.now() - startTime < maxDuration) {
      iteration++;
      await new Promise(r => setTimeout(r, 1500));

      let currentGame;
      try {
        currentGame = await playerBattleship.getGame(gameId);
      } catch (e) {
        console.error('[Test] Error reading game state:', e);
        continue;
      }

      if (!currentGame || currentGame.phase?.type !== "Playing") {
        console.log(`[Test] Game ended (phase: ${currentGame?.phase?.type})`);
        break;
      }

      const playing = currentGame.phase.value;
      if (!playing) continue;

      const currentTurn = playing.current_turn?.type;
      const pendingAttack = playing.pending_attack;
      const round = playing.round ? Number(playing.round) : 0;
      const isPlayerTurn = currentTurn === "Player2";

      if (iteration % 5 === 1) {
        console.log(`[Test] R${round} Turn=${currentTurn} Pending=${pendingAttack ? `(${pendingAttack.x},${pendingAttack.y})` : '-'}`);
      }

      // Player is defender: reveal the attacked cell
      if (pendingAttack && !isPlayerTurn) {
        const { x, y } = pendingAttack;
        const coordKey = `${x},${y}`;

        // Skip duplicate reveals for the same coordinate
        if (lastRevealCoord === coordKey) continue;

        const index = y * 10 + x;
        const cell = playerGrid[index];
        const proof = playerProofs[index];

        console.log(`[Test] Player revealing (${x},${y}), hit=${cell.isOccupied}`);
        try {
          await playerBattleship.revealCell(
            playerAccount.signer, gameId, cell, proof, { x, y }, round
          );
          lastRevealCoord = coordKey;
        } catch (e) {
          console.error('[Test] Reveal error:', e);
        }
        await new Promise(r => setTimeout(r, 1500));
        continue;
      }

      // Clear reveal tracking when no pending attack
      if (!pendingAttack) {
        lastRevealCoord = "";
      }

      // Player's turn to attack
      if (!pendingAttack && isPlayerTurn && lastPlayerRound < round) {
        const target = selectAttackTarget(playerAttacks);
        console.log(`[Test] Player attacking (${target.x},${target.y}) round=${round}`);
        try {
          await playerBattleship.attack(
            playerAccount.signer, gameId, target.x, target.y, round
          );
          playerAttacks.set(`${target.x},${target.y}`, false);
          lastPlayerRound = round;
        } catch (e) {
          console.error('[Test] Attack error:', e);
        }
        await new Promise(r => setTimeout(r, 2000));
        continue;
      }

      // Waiting for bot's action
      await new Promise(r => setTimeout(r, 1000));
    }

    // Verify game progressed
    const finalGame = await playerBattleship.getGame(gameId);
    const finalPhase = finalGame?.phase;
    const finalRound = finalPhase?.type === "Playing" ? Number(finalPhase.value?.round || 0) :
                       finalPhase?.type === "Finished" ? -1 : 0;

    console.log(`\n[Test] Final state: ${finalPhase?.type}`);
    console.log(`[Test] Final round: ${finalRound}`);

    expect(finalGame).toBeTruthy();
    // Game should have progressed past round 0 or finished
    expect(finalRound !== 0).toBe(true);
  }, 180000);
});
