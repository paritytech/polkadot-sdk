import { BattleshipClient } from './battleship.js';
import { createRandomAccount } from './accounts.js';
import { placeShipsRandomly } from './game.js';
import { buildMerkleTree } from './merkle.js';
import { getClient } from './client.js';

async function workingTest() {
  console.log('\n========================================');
  console.log('Bot Integration Test');
  console.log('========================================\n');

  const client = await getClient();
  const battleshipClient = await BattleshipClient.create(client);

  // Create a random account and request funds
  const alice = createRandomAccount();
  console.log(`[Test] Alice address: ${alice.address}`);
  await battleshipClient.requestFunds(alice.address);
  await new Promise(r => setTimeout(r, 6000));

  // Find bot's waiting game
  console.log('[1/6] Finding bot\'s waiting game...');
  const waitingGames = await battleshipClient.findWaitingGames();
  if (waitingGames.length === 0) {
    throw new Error('No waiting game found');
  }
  const gameId = waitingGames[0];
  console.log(`      ✓ Found game ${gameId}\n`);

  // Join game
  console.log('[2/6] Joining game...');
  const joinSuccess = await battleshipClient.joinGame(alice.signer, gameId);
  if (!joinSuccess) throw new Error('Failed to join');
  console.log('      ✓ Joined\n');

  // Place ships and commit
  console.log('[3/6] Placing ships and committing...');
  const aliceGrid = placeShipsRandomly();
  const { root: aliceRoot, proofs: aliceProofs } = buildMerkleTree(aliceGrid);
  const commitSuccess = await battleshipClient.commitGrid(alice.signer, gameId, aliceRoot);
  if (!commitSuccess) throw new Error('Failed to commit');
  console.log('      ✓ Committed\n');

  // Wait for game to start
  console.log('[4/6] Waiting for game to start...');
  let game: any;
  for (let i = 0; i < 40; i++) {
    await new Promise(r => setTimeout(r, 1500));
    game = await battleshipClient.getGame(gameId);
    if (game && game.phase?.type === "Playing") {
      console.log('      ✓ Started\n');
      break;
    }
  }

  // Play rounds
  console.log('[5/6] Playing rounds...');
  for (let turnNum = 1; turnNum <= 5; turnNum++) {
    console.log(`\n   Turn ${turnNum}:`);
    
    await new Promise(r => setTimeout(r, 2000));
    game = await battleshipClient.getGame(gameId);
    
    if (!game || game.phase?.type !== "Playing") {
      console.log('   Game ended');
      break;
    }

    const playingPhase = game.phase.value;
    const pendingAttack = playingPhase?.pending_attack;

    // Check if Alice needs to reveal
    if (pendingAttack && playingPhase?.current_turn?.type === "Player2") {
      const { x, y } = pendingAttack;
      const index = y * 10 + x;
      const cell = aliceGrid[index];
      const proof = aliceProofs[index];
      const round = Number(playingPhase.round || 0);

      console.log(`   - Alice revealing (${x}, ${y}): ${cell.isOccupied ? 'HIT ○' : 'MISS ×'}`);
      await battleshipClient.revealCell(alice.signer, gameId, cell, proof, { x, y }, round);
      await new Promise(r => setTimeout(r, 3000));
      
      game = await battleshipClient.getGame(gameId);
    }

    // Check if it's Alice's turn to attack
    await new Promise(r => setTimeout(r, 1500));
    game = await battleshipClient.getGame(gameId);
    
    if (!game || game.phase?.type !== "Playing") continue;
    
    const phase2 = game.phase.value;
    if (!phase2?.pending_attack && phase2?.current_turn?.type === "Player2") {
      const targetX = Math.floor(Math.random() * 10);
      const targetY = Math.floor(Math.random() * 10);
      const round = Number(phase2.round || 0);
      
      console.log(`   - Alice attacking (${targetX}, ${targetY})`);
      await battleshipClient.attack(alice.signer, gameId, targetX, targetY, round);
      await new Promise(r => setTimeout(r, 4000));
    } else {
      console.log(`   - Waiting for bot (turn: ${phase2?.current_turn?.type})`);
      await new Promise(r => setTimeout(r, 3000));
    }
  }

  // Check final state
  console.log('\n[6/6] Checking results...');
  await new Promise(r => setTimeout(r, 2000));
  const finalGame = await battleshipClient.getGame(gameId);
  
  if (!finalGame) throw new Error('Game disappeared');
  
  const finalRound = finalGame.phase?.value?.round || 0;
  console.log(`      Round reached: ${finalRound}`);
  console.log(`      Phase: ${finalGame.phase?.type}\n`);

  if (finalRound < 2) {
    throw new Error(`Game didn't progress enough (only round ${finalRound})`);
  }
  
  console.log('========================================');
  console.log(`✓ TEST PASSED - Reached round ${finalRound}`);
  console.log('========================================\n');
  process.exit(0);
}

workingTest().catch(err => {
  console.error('\n✗ TEST FAILED:', err.message);
  process.exit(1);
});
