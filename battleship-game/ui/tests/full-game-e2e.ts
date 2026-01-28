import { createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/node";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  DEV_PHRASE,
  entropyToMiniSecret,
  mnemonicToEntropy,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner, PolkadotSigner } from "polkadot-api/signer";
import { Binary } from "polkadot-api";
import { firstValueFrom, filter, tap } from "rxjs";
import {
  buildMerkleTree,
  generateProof,
  createChainCells,
  coordToIndex,
  verifyProof,
  getCellLeafHash,
} from "../src/chain/merkle.ts";

const WS_URL = "ws://localhost:36647";

function createSigner(path: string): PolkadotSigner {
  const entropy = mnemonicToEntropy(DEV_PHRASE);
  const miniSecret = entropyToMiniSecret(entropy);
  const derive = sr25519CreateDerive(miniSecret);
  const keyPair = derive(path);
  return getPolkadotSigner(keyPair.publicKey, "Sr25519", (input) =>
    keyPair.sign(input)
  );
}

async function submitAndWaitBestBlock(
  tx: any,
  signer: PolkadotSigner,
  label: string
): Promise<any> {
  const startTime = Date.now();
  console.log(`[${label}] Submitting... (t=0ms)`);
  const observable = tx.signSubmitAndWatch(signer, {
    mortality: { mortal: true, period: 256 },
  });

  const result = await firstValueFrom(
    observable.pipe(
      tap((e: any) => {
        if (e.type !== "txBestBlocksState") {
          console.log(`[${label}] ${e.type} (t=${Date.now() - startTime}ms)`);
        }
      }),
      filter((e: any) => e.type === "txBestBlocksState" && e.found === true)
    )
  );
  console.log(`[${label}] Included in block #${result.block?.number} (t=${Date.now() - startTime}ms)`);
  return result;
}

interface Player {
  name: string;
  signer: PolkadotSigner;
  address: string;
}

const PLAYERS: Record<string, Player> = {
  Charlie: { name: "Charlie", signer: createSigner("//Charlie"), address: "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y" },
  Dave: { name: "Dave", signer: createSigner("//Dave"), address: "5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy" },
  Eve: { name: "Eve", signer: createSigner("//Eve"), address: "5HGjWAeFDfFCWPsjFQdVV2Msvz2XtMktvgocEZcCj68kUMaw" },
  Ferdie: { name: "Ferdie", signer: createSigner("//Ferdie"), address: "5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL" },
};

async function test() {
  console.log("=".repeat(60));
  console.log("FULL BATTLESHIP GAME E2E TEST");
  console.log("=".repeat(60));

  const provider = getWsProvider(WS_URL);
  const client = createClient(withPolkadotSdkCompat(provider));
  const api = client.getUnsafeApi();

  async function cleanupPlayer(player: Player): Promise<boolean> {
    for (let attempt = 0; attempt < 3; attempt++) {
      const existingId = await api.query.Battleship.PlayerGame.getValue(player.address, { at: "best" });
      if (existingId === undefined || existingId === null) return true;
      
      const game = await api.query.Battleship.Games.getValue(existingId, { at: "best" });
      if (!game) return true;
      
      console.log(`${player.name} in game #${existingId}, phase: ${game.phase?.type}`);
      if (game.phase?.type === "Finished") return true;
      if (game.phase?.type === "WaitingForOpponent") return false;
      
      try {
        const tx = api.tx.Battleship.surrender({ game_id: existingId });
        const result = await submitAndWaitBestBlock(tx, player.signer, `${player.name}:surrender`);
        if (!result.ok) {
          console.log(`${player.name} surrender dispatch failed:`, result.dispatchError);
        }
        await new Promise(r => setTimeout(r, 1000));
      } catch (e) {
        console.log(`${player.name} surrender attempt ${attempt + 1} failed:`, e);
      }
    }
    return false;
  }

  async function findFreePlayers(): Promise<[Player, Player]> {
    const pairs = [["Charlie", "Dave"], ["Eve", "Ferdie"], ["Charlie", "Eve"], ["Dave", "Ferdie"]];
    for (const [n1, n2] of pairs) {
      const p1 = PLAYERS[n1], p2 = PLAYERS[n2];
      if (await cleanupPlayer(p1) && await cleanupPlayer(p2)) {
        return [p1, p2];
      }
    }
    throw new Error("No free player pairs available");
  }

  const [player1, player2] = await findFreePlayers();
  console.log(`Using players: ${player1.name} vs ${player2.name}`);

  // Ships must not touch each other (including diagonally)
  // P1 ships (17 total cells):
  // - 5-cell carrier: row 0, cols 0-4 (indices 0-4)
  // - 4-cell battleship: row 2, cols 0-3 (indices 20-23)
  // - 3-cell cruiser: row 4, cols 0-2 (indices 40-42)
  // - 3-cell submarine: row 6, cols 0-2 (indices 60-62)
  // - 2-cell destroyer: row 8, cols 0-1 (indices 80-81)
  const p1ShipIndices = new Set([0, 1, 2, 3, 4, 20, 21, 22, 23, 40, 41, 42, 60, 61, 62, 80, 81]);
  
  // P2 ships - placed on the right side
  // - 5-cell carrier: row 0, cols 5-9 (indices 5-9)
  // - 4-cell battleship: row 2, cols 5-8 (indices 25-28)
  // - 3-cell cruiser: row 4, cols 5-7 (indices 45-47)
  // - 3-cell submarine: row 6, cols 5-7 (indices 65-67)
  // - 2-cell destroyer: row 8, cols 5-6 (indices 85-86)
  const p2ShipIndices = new Set([5, 6, 7, 8, 9, 25, 26, 27, 28, 45, 46, 47, 65, 66, 67, 85, 86]);

  const p1Cells = createChainCells(p1ShipIndices);
  const p2Cells = createChainCells(p2ShipIndices);
  const p1Tree = buildMerkleTree(p1Cells);
  const p2Tree = buildMerkleTree(p2Cells);

  let gameId: bigint;

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 1: CREATE GAME");
  console.log("=".repeat(60));

  {
    const tx = api.tx.Battleship.create_game({ pot_amount: 1000000000000n });
    const result = await submitAndWaitBestBlock(tx, player1.signer, `${player1.name}:create_game`);
    if (!result.ok) throw new Error(`create_game failed: ${JSON.stringify(result.dispatchError)}`);
    const event = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "GameCreated");
    gameId = event?.value?.value?.game_id;
    console.log(`✓ Game created: #${gameId}`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 2: JOIN GAME");
  console.log("=".repeat(60));

  {
    const tx = api.tx.Battleship.join_game({ game_id: gameId });
    const result = await submitAndWaitBestBlock(tx, player2.signer, `${player2.name}:join_game`);
    if (!result.ok) throw new Error(`join_game failed: ${JSON.stringify(result.dispatchError)}`);
    console.log(`✓ ${player2.name} joined`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 3: COMMIT GRIDS");
  console.log("=".repeat(60));

  {
    const tx = api.tx.Battleship.commit_grid({ game_id: gameId, grid_root: Binary.fromBytes(p1Tree.root) });
    const result = await submitAndWaitBestBlock(tx, player1.signer, `${player1.name}:commit`);
    if (!result.ok) throw new Error(`commit failed: ${JSON.stringify(result.dispatchError)}`);
    console.log(`✓ ${player1.name} committed`);
  }

  {
    const tx = api.tx.Battleship.commit_grid({ game_id: gameId, grid_root: Binary.fromBytes(p2Tree.root) });
    const result = await submitAndWaitBestBlock(tx, player2.signer, `${player2.name}:commit`);
    if (!result.ok) throw new Error(`commit failed: ${JSON.stringify(result.dispatchError)}`);
    console.log(`✓ ${player2.name} committed`);
  }

  {
    const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
    if (game.phase.type !== "Playing") throw new Error(`Expected Playing, got ${game.phase.type}`);
    console.log("✓ Battle started!");
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 4: BATTLE");
  console.log("=".repeat(60));

  const p2ShipCoords = [
    { x: 5, y: 0 }, { x: 6, y: 0 }, { x: 7, y: 0 }, { x: 8, y: 0 }, { x: 9, y: 0 },
    { x: 5, y: 2 }, { x: 6, y: 2 }, { x: 7, y: 2 }, { x: 8, y: 2 },
    { x: 5, y: 4 }, { x: 6, y: 4 }, { x: 7, y: 4 },
    { x: 5, y: 6 }, { x: 6, y: 6 }, { x: 7, y: 6 },
    { x: 5, y: 8 }, { x: 6, y: 8 },
  ];

  const p1ShipCoords = [
    { x: 0, y: 0 }, { x: 1, y: 0 }, { x: 2, y: 0 }, { x: 3, y: 0 }, { x: 4, y: 0 },
    { x: 0, y: 2 }, { x: 1, y: 2 }, { x: 2, y: 2 }, { x: 3, y: 2 },
    { x: 0, y: 4 }, { x: 1, y: 4 }, { x: 2, y: 4 },
    { x: 0, y: 6 }, { x: 1, y: 6 }, { x: 2, y: 6 },
    { x: 0, y: 8 }, { x: 1, y: 8 },
  ];

  let p1Hits = 0, p2Hits = 0, turn = 0;
  let isP1Turn = true;

  while (p1Hits < 17 && p2Hits < 17) {
    turn++;
    
    const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
    if (game.phase.type !== "Playing") {
      console.log(`Unexpected phase: ${game.phase.type}`);
      break;
    }
    
    if (game.phase.value.pending_attack) {
      console.log(`WARNING: There's a pending attack at (${game.phase.value.pending_attack.x}, ${game.phase.value.pending_attack.y})`);
    }
    
    isP1Turn = game.phase.value.current_turn.type === "Player1";
    const attacker = isP1Turn ? player1 : player2;
    const defender = isP1Turn ? player2 : player1;
    const defCells = isP1Turn ? p2Cells : p1Cells;
    const defTree = isP1Turn ? p2Tree : p1Tree;
    const targetCoords = isP1Turn ? p2ShipCoords : p1ShipCoords;
    const hitsSoFar = isP1Turn ? p1Hits : p2Hits;
    const target = targetCoords[hitsSoFar];

    console.log(`\n--- Turn ${turn}: ${attacker.name} attacks (${target.x}, ${target.y}) ---`);

    {
      const tx = api.tx.Battleship.attack({ game_id: gameId, coordinate: { x: target.x, y: target.y } });
      const result = await submitAndWaitBestBlock(tx, attacker.signer, `${attacker.name}:attack`);
      if (!result.ok) {
        const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
        const p1GameId = await api.query.Battleship.PlayerGame.getValue(player1.address, { at: "best" });
        const p2GameId = await api.query.Battleship.PlayerGame.getValue(player2.address, { at: "best" });
        console.error("Attack failed!");
        console.error(`  Our gameId: ${gameId}`);
        console.error(`  ${player1.name} in game: ${p1GameId}`);
        console.error(`  ${player2.name} in game: ${p2GameId}`);
        console.error(`  Game #${gameId} state:`, game?.phase);
        console.error(`  Dispatch error:`, result.dispatchError);
        throw new Error("Attack failed");
      }
      
      const gameEndedEvent = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "GameEnded");
      if (gameEndedEvent) {
        console.error("Attack ended game:", gameEndedEvent.value?.value);
        throw new Error("Game unexpectedly ended during attack");
      }
    }

    {
      const index = coordToIndex(target.x, target.y);
      const cell = defCells[index];
      const proof = generateProof(defTree, index);
      const leafHash = getCellLeafHash(cell);
      const localOk = verifyProof(defTree.root, proof, 100, index, leafHash);
      
      console.log(`  Reveal: idx=${index}, occupied=${cell.isOccupied}, localOk=${localOk}`);

      const tx = api.tx.Battleship.reveal_cell({
        game_id: gameId,
        reveal: {
          cell: { salt: Binary.fromBytes(cell.salt), is_occupied: cell.isOccupied },
          proof: proof.map((p) => Binary.fromBytes(p)),
        },
      });
      const result = await submitAndWaitBestBlock(tx, defender.signer, `${defender.name}:reveal`);
      if (!result.ok) {
        const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
        console.error("Reveal failed!");
        console.error(`  Game state: ${game ? JSON.stringify(game.phase) : 'deleted'}`);
        console.error(`  Dispatch error:`, result.dispatchError);
        const allEvents = result.events.filter((e: any) => e.type === "Battleship");
        for (const evt of allEvents) {
          console.error(`  Event: ${evt.value?.type}`, evt.value?.value);
        }
        throw new Error("Reveal failed");
      }

      const revealEvent = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "AttackRevealed");
      const gameEndedEvent = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "GameEnded");
      const hit = revealEvent?.value?.value?.hit;
      console.log(`  Result: ${hit ? "HIT!" : "miss"}`);
      if (gameEndedEvent) {
        console.log(`  GAME ENDED:`, gameEndedEvent.value?.value);
      }

      if (hit) {
        if (isP1Turn) p1Hits++; else p2Hits++;
      }
    }

    {
      const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
      if (!game) { console.log("  Game deleted!"); break; }
      console.log(`  Phase: ${game.phase.type}, P1 hits: ${p1Hits}, P2 hits: ${p2Hits}`);
      if (game.phase.type === "PendingWinnerReveal") { console.log("\n✓ All ships sunk!"); break; }
      if (game.phase.type === "Playing") {
        isP1Turn = game.phase.value.current_turn.type === "Player1";
      }
    }
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 5: WINNER REVEALS GRID");
  console.log("=".repeat(60));

  {
    const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
    if (game.phase.type !== "PendingWinnerReveal") throw new Error(`Expected PendingWinnerReveal, got ${game.phase.type}`);
    
    const winnerRole = game.phase.value.winner.type;
    const winner = winnerRole === "Player1" ? player1 : player2;
    const winnerCells = winnerRole === "Player1" ? p1Cells : p2Cells;
    
    console.log(`Winner: ${winner.name}`);
    
    const tx = api.tx.Battleship.reveal_winner_grid({
      game_id: gameId,
      full_grid: winnerCells.map((c) => ({ salt: Binary.fromBytes(c.salt), is_occupied: c.isOccupied })),
    });
    const result = await submitAndWaitBestBlock(tx, winner.signer, `${winner.name}:reveal_grid`);
    if (!result.ok) throw new Error("Reveal grid failed");
    console.log(`✓ ${winner.name} revealed grid`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 6: VERIFY FINISHED");
  console.log("=".repeat(60));

  {
    const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
    if (game !== undefined) {
      throw new Error(`Expected game to be removed after valid win, but game still exists with phase: ${game.phase.type}`);
    }
    const p1InGame = await api.query.Battleship.PlayerGame.getValue(player1.address, { at: "best" });
    const p2InGame = await api.query.Battleship.PlayerGame.getValue(player2.address, { at: "best" });
    if (p1InGame !== undefined || p2InGame !== undefined) {
      throw new Error(`Expected player mappings to be cleared`);
    }
    console.log(`✓ Game finished and cleaned up!`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("ALL TESTS PASSED!");
  console.log("=".repeat(60));

  client.destroy();
  process.exit(0);
}

test().catch((e) => {
  console.error("\nTEST FAILED:", e);
  process.exit(1);
});
