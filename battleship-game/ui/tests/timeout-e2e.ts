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
  console.log("TIMEOUT E2E TEST");
  console.log("=".repeat(60));

  const provider = getWsProvider(WS_URL);
  const client = createClient(withPolkadotSdkCompat(provider));
  const api = client.getUnsafeApi();

  const player1 = PLAYERS.Eve;
  const player2 = PLAYERS.Ferdie;

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
  console.log("PHASE 3: PLAYER 1 ATTACKS, PLAYER 2 DOESN'T REVEAL");
  console.log("=".repeat(60));

  const attackCoord = { x: 5, y: 0 };
  let lastActionBlock: bigint;

  {
    const tx = api.tx.Battleship.attack({ game_id: gameId, coordinate: attackCoord });
    const result = await submitAndWaitBestBlock(tx, player1.signer, `${player1.name}:attack`);
    if (!result.ok) throw new Error(`attack failed: ${JSON.stringify(result.dispatchError)}`);
    
    const game = await api.query.Battleship.Games.getValue(gameId, { at: "best" });
    lastActionBlock = game.last_action_block;
    console.log(`✓ ${player1.name} attacked (${attackCoord.x}, ${attackCoord.y}), last_action_block: ${lastActionBlock}`);
    console.log(`  Pending attack:`, game.phase?.value?.pending_attack);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 4: WAIT FOR TIMEOUT AND CLAIM WIN");
  console.log("=".repeat(60));

  const turnTimeout = await api.constants.Battleship.TurnTimeout();
  console.log(`TurnTimeout is ${turnTimeout} blocks`);

  console.log("Waiting for timeout...");
  
  let currentBlock: bigint;
  const timeoutBlock = lastActionBlock + turnTimeout;
  
  while (true) {
    currentBlock = await api.query.System.Number.getValue({ at: "best" });
    if (currentBlock >= timeoutBlock) {
      console.log(`✓ Timeout reached! Current block: ${currentBlock}, timeout was at: ${timeoutBlock}`);
      break;
    }
    const blocksLeft = timeoutBlock - currentBlock;
    process.stdout.write(`\r  Waiting... current: ${currentBlock}, need: ${timeoutBlock} (${blocksLeft} blocks left)   `);
    await new Promise(r => setTimeout(r, 500));
  }
  console.log();

  {
    const tx = api.tx.Battleship.claim_timeout_win({ game_id: gameId });
    const result = await submitAndWaitBestBlock(tx, player1.signer, `${player1.name}:claim_timeout`);
    if (!result.ok) throw new Error(`claim_timeout_win failed: ${JSON.stringify(result.dispatchError)}`);
    
    const gameEndedEvent = result.events.find((e: any) => e.type === "Battleship" && e.value?.type === "GameEnded");
    if (!gameEndedEvent) throw new Error("Expected GameEnded event");
    
    const reason = gameEndedEvent.value?.value?.reason?.type;
    console.log(`✓ ${player1.name} claimed timeout win! Reason: ${reason}`);
  }

  console.log("\n" + "=".repeat(60));
  console.log("PHASE 5: VERIFY GAME ENDED");
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
  console.log("TIMEOUT TEST PASSED!");
  console.log("=".repeat(60));

  client.destroy();
  process.exit(0);
}

test().catch((e) => {
  console.error("\nTEST FAILED:", e);
  process.exit(1);
});
