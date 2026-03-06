import { BattleshipClient } from './battleship.js';
import { aliceAccount } from './accounts.js';
import { placeShipsRandomly } from './game.js';
import { buildMerkleTree } from './merkle.js';
import { getClient } from './client.js';

async function robustTest(runNumber: number): Promise<void> {
  const sep = '='.repeat(50);
  console.log(`\n${sep}`);
  console.log(`Test Run #${runNumber}`);
  console.log(sep);

  const client = await getClient();
  const battleshipClient = await BattleshipClient.create(client);
  const alice = aliceAccount;

  // Find bot's waiting game (with retries)
  console.log('[1/7] Finding bot game...');
  let waitingGames: bigint[] = [];
  for (let attempt = 0; attempt < 10; attempt++) {
    waitingGames = await battleshipClient.findWaitingGames();
    if (waitingGames.length > 0) break;
    console.log(`      Waiting for bot to create game (${attempt + 1}/10)...`);
    await new Promise(r => setTimeout(r, 3000));
  }
  
  if (waitingGames.length === 0) {
    throw new Error('No waiting game found after 30s');
  }
  
  const gameId = waitingGames[0];
  console.log(`      ✓ Found game ${gameId}`);

  // Join game
  console.log('[2/7] Joining game...');
  const joinSuccess = await battleshipClient.joinGame(alice.signer, gameId);
  if (!joinSuccess) throw new Error('Failed to join');
  console.log('      ✓ Joined');

  // Place ships and commit
  console.log('[3/7] Placing ships and committing...');
  const aliceGrid = placeShipsRandomly();
  const { root: aliceRoot, proofs: aliceProofs } = buildMerkleTree(aliceGrid);
  const commitSuccess = await battleshipClient.commitGrid(alice.signer, gameId, aliceRoot);
  if (!commitSuccess) throw new Error('Failed to commit');
  console.log('      ✓ Committed');

  // Wait for game to start
  console.log('[4/7] Waiting for game to start...');
  let game: any;
  for (let i = 0; i < 40; i++) {
    await new Promise(r => setTimeout(r, 1500));
    game = await battleshipClient.getGame(gameId);
    if (game && game.phase?.type === "Playing") {
      console.log('      ✓ Started');
      break;
    }
  }
  
  if (!game || game.phase?.type !== "Playing") {
    throw new Error('Game did not start');
  }

  // Determine if Alice is Player1 or Player2
  const aliceIsPlayer1 = game.player1?.toString() === alice.address;
  console.log(`[5/7] Alice is Player${aliceIsPlayer1 ? '1' : '2'}`);

  console.log('[6/7] Playing 3 turns...');
  let completedTurns = 0;
  let lastRevealCoord = "";
  const attackedCoords = new Set<string>();

  try {
    for (let attempt = 0; attempt < 60 && completedTurns < 3; attempt++) {
      await new Promise(r => setTimeout(r, 2000));

      game = await battleshipClient.getGame(gameId);
      if (!game || game.phase?.type !== "Playing") {
        console.log('      Game ended');
        break;
      }

      const phase = game.phase.value;
      if (!phase) continue;

      const currentTurn = phase.current_turn?.type;
      const pendingAttack = phase.pending_attack;
      const round = Number(phase.round || 0);
      const aliceTurn = aliceIsPlayer1 ? "Player1" : "Player2";
      const isAliceTurn = currentTurn === aliceTurn;

      if (pendingAttack && !isAliceTurn) {
        const { x, y } = pendingAttack;
        const coordKey = `${x},${y}`;

        if (lastRevealCoord === coordKey) continue;

        const index = y * 10 + x;
        const cell = aliceGrid[index];
        const proof = aliceProofs[index];

        console.log(`      Reveal: (${x},${y}) ${cell.isOccupied ? 'HIT' : 'MISS'} round=${round}`);
        await battleshipClient.revealCell(alice.signer, gameId, cell, proof, { x, y }, round);
        lastRevealCoord = coordKey;
        completedTurns++;
        await new Promise(r => setTimeout(r, 3000));
        continue;
      }

      if (!pendingAttack) lastRevealCoord = "";

      if (!pendingAttack && isAliceTurn) {
        let targetX: number, targetY: number, coordKey: string;
        do {
          targetX = Math.floor(Math.random() * 10);
          targetY = Math.floor(Math.random() * 10);
          coordKey = `${targetX},${targetY}`;
        } while (attackedCoords.has(coordKey));
        attackedCoords.add(coordKey);

        console.log(`      Attack: (${targetX},${targetY}) round=${round}`);
        await battleshipClient.attack(alice.signer, gameId, targetX, targetY, round);
        await new Promise(r => setTimeout(r, 3000));
        continue;
      }

      if (attempt % 5 === 0) {
        console.log(`      Waiting for bot... (attempt ${attempt})`);
      }
    }

    console.log('[7/7] Verifying...');
    await new Promise(r => setTimeout(r, 2000));
    const finalGame = await battleshipClient.getGame(gameId);

    if (!finalGame) throw new Error('Game disappeared');

    const finalRound = finalGame.phase?.value?.round || 0;
    console.log(`      Final round: ${finalRound}, turns: ${completedTurns}`);

    if (completedTurns < 2) {
      throw new Error(`Only completed ${completedTurns} turns, expected at least 2`);
    }

    console.log(`      ✓ Completed ${completedTurns} turns successfully`);
    console.log(`✓ Run #${runNumber} PASSED`);
  } finally {
    console.log(`      Surrendering game ${gameId}...`);
    await battleshipClient.surrender(alice.signer, gameId).catch(() => {});
    await new Promise(r => setTimeout(r, 3000));
  }
}

async function main() {
  const totalRuns = parseInt(process.argv[2] || '20');
  const sep = '='.repeat(50);
  console.log(`\n${sep}`);
  console.log(`Running ${totalRuns} test iterations`);
  console.log(sep + '\n');

  let passed = 0;
  let failed = 0;

  for (let i = 1; i <= totalRuns; i++) {
    try {
      await robustTest(i);
      passed++;
    } catch (err: any) {
      console.error(`\n✗ Run #${i} FAILED: ${err.message}`);
      failed++;
    }
    
    // Brief pause between runs
    if (i < totalRuns) {
      await new Promise(r => setTimeout(r, 2000));
    }
  }

  console.log(`\n${sep}`);
  console.log(`Final Results:`);
  console.log(`  Passed: ${passed}/${totalRuns}`);
  console.log(`  Failed: ${failed}/${totalRuns}`);
  console.log(`  Success Rate: ${((passed / totalRuns) * 100).toFixed(1)}%`);
  console.log(sep + '\n');

  if (failed > 0) {
    process.exit(1);
  }
  
  process.exit(0);
}

main();
