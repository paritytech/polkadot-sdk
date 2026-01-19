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
  createChainCells,
  coordToIndex,
  generateProof,
} from "../src/chain/merkle.ts";

const WS_URL = "ws://localhost:37359";

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
  console.log("INVALID REVEAL E2E TEST (CHEATING DETECTION)");
  console.log("=".repeat(60));

  const provider = getWsProvider(WS_URL);
  const client = createClient(withPolkadotSdkCompat(provider));
  const api = client.getUnsafeApi();

  // Use Charlie and Dave for this test
  const player1 = PLAYERS.Charlie;
  const player2 = PLAYERS.Dave;

  // Ship placement on rows 0, 2, 4, 6, 8 to avoid adjacent ship detection
  const p1ShipIndices = new Set([0, 1, 2, 3, 4, 20, 21, 22, 23, 40, 41, 42, 60, 61, 62, 80, 81]);
  const p2ShipIndices = new Set([5, 6, 7, 8, 9, 25, 26, 27, 28, 45, 46, 47, 65, 66, 67, 85, 86]);

  const p1Cells = createChainCells(p1ShipIndices);
  const p2Cells = createChainCells(p2ShipIndices);
  const p1Tree = buildMerkleTree(p1Cells);
  const p2Tree = buildMerkleTree(p2Cells);

  let gameId: bigint;

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 1: CREATE AND JOIN GAME");
  console.log("=".repeat(60));

  {
    const tx = api.tx.Battleship.create_game({ pot_amount: 1000000000000n });
    const result = await submitAndWaitBestBlock(tx, player1.signer, `${player1.name}:create_game`);
    if (!result.ok) throw new Error(`create_game failed: ${JSON.stringify(result.dispatchError)}`);
    const event = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "GameCreated");
    gameId = event?.value?.value?.game_id;
    console.log(`✓ Game created: #${gameId}`);
  }

  {
    const tx = api.tx.Battleship.join_game({ game_id: gameId });
    const result = await submitAndWaitBestBlock(tx, player2.signer, `${player2.name}:join_game`);
    if (!result.ok) throw new Error(`join_game failed: ${JSON.stringify(result.dispatchError)}`);
    console.log(`✓ ${player2.name} joined`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 2: COMMIT GRIDS");
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

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 3: PLAYER 1 ATTACKS");
  console.log("=".repeat(60));

  // Player 1 attacks at (5, 0) which is a ship cell for player 2
  const attackCoord = { x: 5, y: 0 };

  {
    const tx = api.tx.Battleship.attack({ game_id: gameId, coordinate: attackCoord });
    const result = await submitAndWaitBestBlock(tx, player1.signer, `${player1.name}:attack`);
    if (!result.ok) throw new Error(`attack failed: ${JSON.stringify(result.dispatchError)}`);
    console.log(`✓ ${player1.name} attacked (${attackCoord.x}, ${attackCoord.y})`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 4: PLAYER 2 SENDS INVALID REVEAL (WRONG SALT)");
  console.log("=".repeat(60));

  {
    const index = coordToIndex(attackCoord.x, attackCoord.y);
    const realCell = p2Cells[index];
    const proof = generateProof(p2Tree, index);
    
    // Create a FAKE salt (different from the committed one)
    const fakeSalt = new Uint8Array(32);
    fakeSalt.fill(0xff); // All 0xff bytes, definitely not the real salt
    
    console.log(`  Real cell: occupied=${realCell.isOccupied}, salt=${Buffer.from(realCell.salt).toString('hex').slice(0, 16)}...`);
    console.log(`  Fake salt: ${Buffer.from(fakeSalt).toString('hex').slice(0, 16)}...`);
    console.log(`  Sending reveal with FAKE salt (should trigger cheating detection)`);
    
    const tx = api.tx.Battleship.reveal_cell({
      game_id: gameId,
      reveal: {
        cell: { salt: Binary.fromBytes(fakeSalt), is_occupied: realCell.isOccupied },
        proof: proof.map((p) => Binary.fromBytes(p)),
      },
    });
    
    const result = await submitAndWaitBestBlock(tx, player2.signer, `${player2.name}:invalid_reveal`);
    
    // The transaction should succeed (it's valid structurally), but the game logic should detect cheating
    if (!result.ok) {
      console.log(`  Transaction failed with dispatch error: ${JSON.stringify(result.dispatchError)}`);
      throw new Error("Expected transaction to succeed but dispatch error occurred");
    }
    
    // Look for GameEnded event with Cheating reason
    const gameEndedEvent = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "GameEnded");
    if (!gameEndedEvent) {
      throw new Error("Expected GameEnded event due to cheating, but none found");
    }
    
    const reason = gameEndedEvent.value?.value?.reason?.type;
    const winner = gameEndedEvent.value?.value?.winner;
    
    console.log(`✓ Game ended! Reason: ${reason}`);
    console.log(`  Winner: ${winner === player1.address ? player1.name : player2.name}`);
    
    if (reason !== "Cheating") {
      throw new Error(`Expected reason 'Cheating', got '${reason}'`);
    }
    
    if (winner !== player1.address) {
      throw new Error(`Expected winner to be ${player1.name} (the attacker), but got someone else`);
    }
    
    console.log(`✓ Cheating correctly detected! ${player1.name} wins because ${player2.name} submitted invalid proof.`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 5: VERIFY GAME CLEANUP");
  console.log("=".repeat(60));

  {
    const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
    if (game !== undefined) {
      throw new Error(`Expected game to be removed, but found: ${JSON.stringify(game.phase)}`);
    }
    console.log("✓ Game removed from storage");

    const p1InGame = await api.query.Battleship.PlayerGame.getValue(player1.address, { at: "best" });
    const p2InGame = await api.query.Battleship.PlayerGame.getValue(player2.address, { at: "best" });
    if (p1InGame !== undefined || p2InGame !== undefined) {
      throw new Error("Expected player mappings to be cleared");
    }
    console.log("✓ Player mappings cleared");
  }

  console.log("\n" + "=".repeat(60));
  console.log("INVALID REVEAL TEST PASSED!");
  console.log("=".repeat(60));

  client.destroy();
  process.exit(0);
}

test().catch((e) => {
  console.error("\nTEST FAILED:", e);
  process.exit(1);
});
