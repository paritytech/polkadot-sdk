import { BattleshipClient } from './battleship.js';
import { aliceAccount } from './accounts.js';
import { placeShipsRandomly } from './game.js';
import { buildMerkleTree } from './merkle.js';
import { getClient } from './client.js';
async function simpleTest() {
    console.log('\n========================================');
    console.log('Simple Bot Test - Playing Against Bot');
    console.log('========================================\n');
    const client = await getClient();
    const battleshipClient = await BattleshipClient.create(client);
    const alice = aliceAccount;
    // Find bot's waiting game
    console.log('[1/7] Looking for bot\'s waiting game...');
    const waitingGames = await battleshipClient.findWaitingGames();
    if (waitingGames.length === 0) {
        throw new Error('No waiting game found. Is the bot running?');
    }
    const gameId = waitingGames[0];
    console.log(`      ✓ Found game ${gameId}\n`);
    // Join game
    console.log('[2/7] Alice joining game...');
    const joinSuccess = await battleshipClient.joinGame(alice.signer, gameId);
    if (!joinSuccess)
        throw new Error('Failed to join game');
    console.log('      ✓ Joined successfully\n');
    // Place ships and commit
    console.log('[3/7] Alice placing ships and committing grid...');
    const aliceGrid = placeShipsRandomly();
    const { root: aliceRoot, proofs: aliceProofs } = buildMerkleTree(aliceGrid);
    const commitSuccess = await battleshipClient.commitGrid(alice.signer, gameId, aliceRoot);
    if (!commitSuccess)
        throw new Error('Failed to commit grid');
    console.log('      ✓ Grid committed\n');
    // Wait for game to start
    console.log('[4/7] Waiting for game to start (bot commits)...');
    let gameStarted = false;
    for (let i = 0; i < 40; i++) {
        await new Promise(r => setTimeout(r, 1500));
        const game = await battleshipClient.getGame(gameId);
        if (game && game.phase?.type === "Playing") {
            console.log('      ✓ Game started!\n');
            gameStarted = true;
            break;
        }
    }
    if (!gameStarted)
        throw new Error('Game did not start in time');
    // Play 3 complete rounds with proper waiting
    console.log('[5/7] Playing 3 complete rounds...');
    for (let round = 1; round <= 3; round++) {
        console.log(`\n   Round ${round}:`);
        // Wait a bit before checking state
        await new Promise(r => setTimeout(r, 3000));
        let game = await battleshipClient.getGame(gameId);
        if (!game || game.phase?.type !== "Playing") {
            console.log('      Game ended');
            break;
        }
        // If there's a pending reveal, handle it
        if (game.pending_reveal) {
            const targetPlayer = game.pending_reveal.target_player?.toString();
            console.log(`   - Pending reveal for ${targetPlayer === alice.address ? 'Alice' : 'Bot'}`);
            if (targetPlayer === alice.address) {
                const { x, y } = game.pending_reveal;
                const index = y * 10 + x;
                const cell = aliceGrid[index];
                const proof = aliceProofs[index];
                const currentRound = Number(game.round_number || 0);
                console.log(`   - Alice revealing (${x}, ${y}): ${cell.isOccupied ? 'HIT' : 'MISS'}`);
                await battleshipClient.revealCell(alice.signer, gameId, cell, proof, { x, y }, currentRound);
                await new Promise(r => setTimeout(r, 3000));
            }
            else {
                // Wait for bot to reveal
                console.log(`   - Waiting for bot to reveal...`);
                for (let i = 0; i < 20; i++) {
                    await new Promise(r => setTimeout(r, 1500));
                    game = await battleshipClient.getGame(gameId);
                    if (!game.pending_reveal) {
                        console.log(`   - Bot revealed`);
                        break;
                    }
                }
            }
        }
        // Now check if it's Alice's turn to attack
        await new Promise(r => setTimeout(r, 2000));
        game = await battleshipClient.getGame(gameId);
        if (!game || game.phase?.type !== "Playing") {
            console.log('      Game ended');
            break;
        }
        if (!game.pending_reveal) {
            // Determine whose turn it is
            const roundNum = Number(game.round_number || 0);
            const currentTurn = game.current_turn;
            // Simple logic: if no pending reveal, try to attack
            const targetX = Math.floor(Math.random() * 10);
            const targetY = Math.floor(Math.random() * 10);
            console.log(`   - Alice attacking (${targetX}, ${targetY})`);
            await battleshipClient.attack(alice.signer, gameId, targetX, targetY, roundNum);
            await new Promise(r => setTimeout(r, 3000));
        }
    }
    // Check final state
    console.log('\n[6/7] Checking final game state...');
    await new Promise(r => setTimeout(r, 2000));
    const finalGame = await battleshipClient.getGame(gameId);
    if (!finalGame) {
        throw new Error('Game disappeared');
    }
    const finalRound = Number(finalGame.round_number || 0);
    console.log(`      Phase: ${finalGame.phase?.type}`);
    console.log(`      Round: ${finalRound}\n`);
    // Verify success
    console.log('[7/7] Verifying test success...');
    if (finalRound < 1) {
        throw new Error(`Game did not progress (round ${finalRound})`);
    }
    console.log(`      ✓ Test PASSED - Game progressed to round ${finalRound}\n`);
    console.log('========================================');
    console.log('✓ ALL TESTS PASSED');
    console.log('========================================\n');
    process.exit(0);
}
simpleTest().catch(err => {
    console.error('\n✗ TEST FAILED:', err.message);
    console.error(err);
    process.exit(1);
});
