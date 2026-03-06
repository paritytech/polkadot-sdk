import { describe, it, expect, beforeAll } from '@jest/globals';
import { BattleshipClient } from '../src/battleship.js';
import { aliceAccount } from '../src/accounts.js';
import { placeShipsRandomly } from '../src/game.js';
import { buildMerkleTree } from '../src/merkle.js';
import { getClient } from '../src/client.js';

/**
 * Test playing against the bot
 * 
 * Prerequisites:
 * - Zombienet running with battleship parachain
 * - Bot already running (npm start in bot directory)
 */
describe('Bot Game Test', () => {
  let battleshipClient: BattleshipClient;
  let gameId: bigint;

  beforeAll(async () => {
    console.log('[Test] Initializing client...');
    const client = await getClient();
    battleshipClient = await BattleshipClient.create(client);
    
    // Wait a bit for initialization
    await new Promise(r => setTimeout(r, 3000));
  }, 30000);

  it('should play a game against the bot', async () => {
    const alice = aliceAccount;

    // Find bot's waiting game
    console.log('[Test] Looking for bot\'s game...');
    const waitingGames = await battleshipClient.findWaitingGames();
    
    if (waitingGames.length === 0) {
      throw new Error('No waiting game found. Is the bot running?');
    }
    
    gameId = waitingGames[0];
    console.log(`[Test] Found waiting game: ${gameId}`);

    // Alice joins the game
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

    // Wait for game to start (bot commits)
    console.log('[Test] Waiting for game to start...');
    let gameStarted = false;
    for (let i = 0; i < 40; i++) {
      await new Promise(r => setTimeout(r, 1500));
      const game = await battleshipClient.getGame(gameId);
      if (game && game.phase?.type === "Playing") {
        console.log('[Test] ✓ Game started!');
        gameStarted = true;
        break;
      }
    }
    expect(gameStarted).toBe(true);

    // Play rounds
    for (let roundNum = 0; roundNum < 10; roundNum++) {
      console.log(`\n[Test] ===== Round ${roundNum + 1} =====`);
      
      await new Promise(r => setTimeout(r, 2000));
      const currentGame = await battleshipClient.getGame(gameId);
      
      if (!currentGame || currentGame.phase?.type !== "Playing") {
        console.log('[Test] Game ended');
        break;
      }

      const pendingReveal = currentGame.pending_reveal;

      // Handle reveal if needed
      if (pendingReveal) {
        const targetPlayer = pendingReveal.target_player?.toString();
        if (targetPlayer === alice.address) {
          const { x, y } = pendingReveal;
          const index = y * 10 + x;
          const cell = aliceGrid[index];
          const proof = aliceProofs[index];
          const currentRound = Number(currentGame.round_number || 0);

          console.log(`[Test] Alice revealing cell (${x}, ${y}), occupied=${cell.isOccupied}`);
          await battleshipClient.revealCell(
            alice.signer,
            gameId,
            cell,
            proof,
            { x, y },
            currentRound
          );
          
          await new Promise(r => setTimeout(r, 2000));
        } else {
          console.log('[Test] Waiting for bot to reveal...');
          await new Promise(r => setTimeout(r, 3000));
          continue;
        }
      }

      // Check if Alice should attack
      await new Promise(r => setTimeout(r, 1000));
      const updatedGame = await battleshipClient.getGame(gameId);
      
      if (!updatedGame?.pending_reveal) {
        const lastAttacker = updatedGame?.last_attacker?.toString();
        const isAliceTurn = !lastAttacker || lastAttacker !== alice.address;

        if (isAliceTurn) {
          const targetX = Math.floor(Math.random() * 10);
          const targetY = Math.floor(Math.random() * 10);
          const attackRound = Number(updatedGame?.round_number || 0);

          console.log(`[Test] Alice attacking (${targetX}, ${targetY})`);
          await battleshipClient.attack(alice.signer, gameId, targetX, targetY, attackRound);
          
          await new Promise(r => setTimeout(r, 3000));
        } else {
          console.log('[Test] Waiting for bot to attack...');
          await new Promise(r => setTimeout(r, 3000));
        }
      }
    }

    // Verify game progressed
    const finalGame = await battleshipClient.getGame(gameId);
    console.log(`\n[Test] Final state: ${finalGame?.phase?.type}`);
    console.log(`[Test] Final round: ${finalGame?.round_number}`);
    
    expect(finalGame).toBeTruthy();
    expect(Number(finalGame?.round_number || 0)).toBeGreaterThan(0);
  }, 180000); // 3 minute timeout
});
