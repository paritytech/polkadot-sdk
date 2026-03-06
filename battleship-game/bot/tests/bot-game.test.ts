import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import { BattleshipClient } from '../src/battleship.js';
import { BattleshipBot } from '../src/bot.js';
import { aliceAccount, botAccount } from '../src/accounts.js';
import { placeShipsRandomly } from '../src/game.js';
import { buildMerkleTree } from '../src/merkle.js';
import { getClient, disconnectClient } from '../src/client.js';

/**
 * Test playing against the bot
 *
 * Prerequisites:
 * - Zombienet running with battleship parachain
 */
describe('Bot Game Test', () => {
  let battleshipClient: BattleshipClient;
  let gameId: bigint;
  let botRunning = false;

  beforeAll(async () => {
    console.log('[Test] Initializing client...');
    const client = await getClient();
    battleshipClient = await BattleshipClient.create(client);

    // Surrender any existing games for both Alice and bot
    for (const account of [aliceAccount, botAccount]) {
      const existingGameId = await battleshipClient.getPlayerGame(account.address);
      if (existingGameId !== null) {
        console.log(`[Test] ${account.address.slice(0, 8)}... is in game ${existingGameId}, surrendering...`);
        await battleshipClient.surrender(account.signer, existingGameId);
        await new Promise(r => setTimeout(r, 6000));
      }
    }

    // Start the bot in the background
    console.log('[Test] Starting bot...');
    const bot = new BattleshipBot(battleshipClient, botAccount);
    botRunning = true;
    bot.run().catch(e => {
      console.error('[Test] Bot error:', e);
      botRunning = false;
    });

    // Wait for bot to create a game
    console.log('[Test] Waiting for bot to create a game...');
    let found = false;
    for (let i = 0; i < 30; i++) {
      await new Promise(r => setTimeout(r, 2000));
      const waitingGames = await battleshipClient.findWaitingGames();
      if (waitingGames.length > 0) {
        found = true;
        break;
      }
    }
    if (!found) {
      throw new Error('Bot did not create a game within timeout');
    }
  }, 120000);

  afterAll(() => {
    disconnectClient();
  });

  it('should play a game against the bot', async () => {
    const alice = aliceAccount;

    // Find bot's waiting game
    console.log('[Test] Looking for bot\'s game...');
    const waitingGames = await battleshipClient.findWaitingGames();
    expect(waitingGames.length).toBeGreaterThan(0);

    gameId = waitingGames[0];
    console.log(`[Test] Found waiting game: ${gameId}`);

    // Alice joins the game (she becomes player2)
    console.log(`[Test] Alice joining game ${gameId}...`);
    const joinSuccess = await battleshipClient.joinGame(alice.signer, gameId);
    expect(joinSuccess).toBe(true);

    // Alice places ships and commits
    console.log('[Test] Alice placing ships...');
    const aliceGrid = placeShipsRandomly();
    const { root: aliceRoot, proofs: aliceProofs } = buildMerkleTree(aliceGrid);

    console.log('[Test] Alice committing grid...');
    const commitSuccess = await battleshipClient.commitGrid(alice.signer, gameId, aliceRoot);
    expect(commitSuccess).toBe(true);

    // Wait for game to start (bot also commits)
    console.log('[Test] Waiting for game to start...');
    let gameStarted = false;
    for (let i = 0; i < 40; i++) {
      await new Promise(r => setTimeout(r, 1500));
      const game = await battleshipClient.getGame(gameId);
      if (game && game.phase?.type === "Playing") {
        console.log('[Test] Game started!');
        gameStarted = true;
        break;
      }
    }
    expect(gameStarted).toBe(true);

    // Play rounds - Alice is player2 (she joined), bot is player1 (created the game)
    const startTime = Date.now();
    const maxDuration = 120_000; // 2 minutes max
    let lastAliceRound = -1;
    let iteration = 0;

    while (Date.now() - startTime < maxDuration) {
      iteration++;
      await new Promise(r => setTimeout(r, 1500));
      const currentGame = await battleshipClient.getGame(gameId);

      if (!currentGame || currentGame.phase?.type !== "Playing") {
        console.log(`[Test] Game ended (phase: ${currentGame?.phase?.type})`);
        break;
      }

      const playingPhase = currentGame.phase.value;
      if (!playingPhase) continue;

      const currentTurn = playingPhase.current_turn?.type;
      const pendingAttack = playingPhase.pending_attack;
      const currentRound = playingPhase.round ? Number(playingPhase.round) : 0;
      const isAliceTurn = currentTurn === "Player2";

      if (iteration % 5 === 1) {
        console.log(`[Test] Round ${currentRound}, Turn: ${currentTurn}, Pending: ${pendingAttack ? `(${pendingAttack.x},${pendingAttack.y})` : 'none'}`);
      }

      // Alice is defender: reveal the attacked cell
      if (pendingAttack && !isAliceTurn) {
        const { x, y } = pendingAttack;
        const index = y * 10 + x;
        const cell = aliceGrid[index];
        const proof = aliceProofs[index];

        console.log(`[Test] Alice revealing (${x},${y}), hit=${cell.isOccupied}`);
        await battleshipClient.revealCell(alice.signer, gameId, cell, proof, { x, y }, currentRound);
        await new Promise(r => setTimeout(r, 1500));
        continue;
      }

      // Alice's turn to attack
      if (!pendingAttack && isAliceTurn && lastAliceRound < currentRound) {
        const targetX = Math.floor(Math.random() * 10);
        const targetY = Math.floor(Math.random() * 10);
        console.log(`[Test] Alice attacking (${targetX},${targetY}) round=${currentRound}`);
        await battleshipClient.attack(alice.signer, gameId, targetX, targetY, currentRound);
        lastAliceRound = currentRound;
        await new Promise(r => setTimeout(r, 2000));
        continue;
      }

      // Waiting for bot
      await new Promise(r => setTimeout(r, 1000));
    }

    // Verify game progressed
    const finalGame = await battleshipClient.getGame(gameId);
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
