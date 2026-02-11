import { createClient, AccountId } from "polkadot-api";
import { getWsProvider } from "@polkadot-api/ws-provider";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import { Bytes, Vector, Struct, u8, u32, u64, bool } from "scale-ts";
import { blake2b } from "@noble/hashes/blake2b";

const PALLET = 0x32;
const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);
const aliceKp = derive("//Alice");
const bobKp = derive("//Bob");
const aliceSigner = getPolkadotSigner(aliceKp.publicKey, "Sr25519", aliceKp.sign);
const bobSigner = getPolkadotSigner(bobKp.publicKey, "Sr25519", bobKp.sign);
const aliceAddr = AccountId().dec(aliceKp.publicKey);
const bobAddr = AccountId().dec(bobKp.publicKey);

async function rpc(method: string, params: unknown[] = []) {
  const resp = await fetch("http://127.0.0.1:36637", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const data = await resp.json();
  if (data.error) throw new Error(`${method}: ${JSON.stringify(data.error)}`);
  return data.result;
}

async function getNonce(addr: string): Promise<number> {
  return (await rpc("system_accountNextIndex", [addr])) as number;
}

function wrapSigner(signer: any, callData: Uint8Array): any {
  return {
    publicKey: signer.publicKey,
    signTx: (_orig: Uint8Array, ...rest: any[]) => signer.signTx(callData, ...rest),
    signBytes: signer.signBytes,
  };
}

async function submitRaw(api: any, dummyTxFactory: () => any, signer: any, callData: Uint8Array, addr: string, label: string) {
  const nonce = await getNonce(addr);
  const wrapped = wrapSigner(signer, callData);
  const dummyTx = dummyTxFactory();
  const signed = await dummyTx.sign(wrapped, { mortality: { mortal: false }, nonce });
  const hex = "0x" + Array.from(signed as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  const txHash = await rpc("author_submitExtrinsic", [hex]);
  console.log(`[${label}] txHash=${txHash}`);
  // Wait for nonce to increment
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    const newNonce = await getNonce(addr);
    if (newNonce > nonce) {
      console.log(`[${label}] included (nonce ${nonce}->${newNonce})`);
      return;
    }
  }
  throw new Error(`[${label}] timeout`);
}

// Check events at best block
async function getLatestEvents(api: any) {
  const events = await api.query.System.Events.getValue({ at: "best" });
  return events;
}

async function main() {
  const provider = getWsProvider("ws://127.0.0.1:36637");
  const client = createClient(provider);
  const api = client.getUnsafeApi() as any;

  // Wait for connection
  console.log("Waiting for best block...");
  const { firstValueFrom } = await import("rxjs");
  await firstValueFrom(client.bestBlocks$ as any);
  console.log("Connected.");

  // Create game
  console.log("\n--- Creating game ---");
  const createTx = api.tx.Battleship.create_game({ pot_amount: 1000000000000n });
  const n1 = await getNonce(aliceAddr);
  const signed1 = await createTx.sign(aliceSigner, { mortality: { mortal: false }, nonce: n1 });
  const hex1 = "0x" + Array.from(signed1 as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  await rpc("author_submitExtrinsic", [hex1]);
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    if (await getNonce(aliceAddr) > n1) break;
  }
  
  // Get game ID from PlayerGame
  const pgKey = await api.query.Battleship.PlayerGame.getKey(aliceAddr);
  const pgHex = await rpc("state_getStorage", [pgKey]) as string;
  const pgBytes = new Uint8Array(pgHex.slice(2).match(/.{2}/g)!.map((b: string) => parseInt(b, 16)));
  const gameId = new DataView(pgBytes.buffer).getBigUint64(0, true);
  console.log("Game ID:", gameId);

  // Join game (Bob)
  console.log("\n--- Joining game ---");
  const joinTx = api.tx.Battleship.join_game({ game_id: gameId });
  const n2 = await getNonce(bobAddr);
  const signed2 = await joinTx.sign(bobSigner, { mortality: { mortal: false }, nonce: n2 });
  const hex2 = "0x" + Array.from(signed2 as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  await rpc("author_submitExtrinsic", [hex2]);
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    if (await getNonce(bobAddr) > n2) break;
  }
  console.log("Joined.");

  // Commit grids
  console.log("\n--- Committing grids ---");
  // Create cells: 100 cells, first 17 occupied
  const cells: { salt: Uint8Array; isOccupied: boolean }[] = [];
  for (let i = 0; i < 100; i++) {
    const salt = new Uint8Array(32);
    crypto.getRandomValues(salt);
    cells.push({ salt, isOccupied: i < 17 });
  }
  
  // Build merkle tree
  function hashCell(cell: { salt: Uint8Array; isOccupied: boolean }): Uint8Array {
    const data = new Uint8Array(33);
    data.set(cell.salt, 0);
    data[32] = cell.isOccupied ? 1 : 0;
    return blake2b(data, { dkLen: 32 });
  }
  
  function buildTree(cells: { salt: Uint8Array; isOccupied: boolean }[]): { root: Uint8Array; leaves: Uint8Array[] } {
    // pad to 128 (next power of 2)
    const leaves = cells.map(hashCell);
    while (leaves.length < 128) {
      leaves.push(new Uint8Array(32)); // zero hash for empty leaves
    }
    
    let level = [...leaves];
    while (level.length > 1) {
      const next: Uint8Array[] = [];
      for (let i = 0; i < level.length; i += 2) {
        const combined = new Uint8Array(64);
        combined.set(level[i], 0);
        combined.set(level[i + 1], 32);
        next.push(blake2b(combined, { dkLen: 32 }));
      }
      level = next;
    }
    return { root: level[0], leaves: cells.map(hashCell) };
  }
  
  const { root: aliceRoot } = buildTree(cells);
  // Use same cells for Bob (simple test)
  const { root: bobRoot } = buildTree(cells);
  
  const aliceRootHex = "0x" + Array.from(aliceRoot, b => b.toString(16).padStart(2, "0")).join("");
  const bobRootHex = "0x" + Array.from(bobRoot, b => b.toString(16).padStart(2, "0")).join("");
  
  // Commit Alice
  const commitTx1 = api.tx.Battleship.commit_grid({ game_id: gameId, grid_root: aliceRootHex });
  const na = await getNonce(aliceAddr);
  const sc1 = await commitTx1.sign(aliceSigner, { mortality: { mortal: false }, nonce: na });
  const hc1 = "0x" + Array.from(sc1 as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  await rpc("author_submitExtrinsic", [hc1]);
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    if (await getNonce(aliceAddr) > na) break;
  }
  console.log("Alice committed.");
  
  // Commit Bob
  const commitTx2 = api.tx.Battleship.commit_grid({ game_id: gameId, grid_root: bobRootHex });
  const nb = await getNonce(bobAddr);
  const sc2 = await commitTx2.sign(bobSigner, { mortality: { mortal: false }, nonce: nb });
  const hc2 = "0x" + Array.from(sc2 as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  await rpc("author_submitExtrinsic", [hc2]);
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    if (await getNonce(bobAddr) > nb) break;
  }
  console.log("Bob committed.");

  // Check game state
  await new Promise(r => setTimeout(r, 3000));
  const gKey = await api.query.Battleship.Games.getKey(gameId);
  const gHex = await rpc("state_getStorage", [gKey]) as string | null;
  console.log("Game exists:", gHex !== null);

  // Attack (0,0) - Alice
  console.log("\n--- Alice attacks (0,0) ---");
  const attackTx = api.tx.Battleship.attack({ game_id: gameId, coordinate: { x: 0, y: 0 }, expected_round: 0 });
  const nAtk = await getNonce(aliceAddr);
  const sAtk = await attackTx.sign(aliceSigner, { mortality: { mortal: false }, nonce: nAtk });
  const hAtk = "0x" + Array.from(sAtk as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  const atkHash = await rpc("author_submitExtrinsic", [hAtk]);
  console.log("Attack txHash:", atkHash);
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    if (await getNonce(aliceAddr) > nAtk) break;
  }
  console.log("Attack included.");

  // Check game after attack
  await new Promise(r => setTimeout(r, 2000));
  const g2Hex = await rpc("state_getStorage", [gKey]) as string | null;
  console.log("Game exists after attack:", g2Hex !== null);

  // Now reveal - Bob
  console.log("\n--- Bob reveals (0,0) ---");
  // Generate proof for cell 0
  function generateProof(cells: { salt: Uint8Array; isOccupied: boolean }[], index: number): Uint8Array[] {
    const leaves = cells.map(hashCell);
    while (leaves.length < 128) {
      leaves.push(new Uint8Array(32));
    }
    
    const proof: Uint8Array[] = [];
    let level = [...leaves];
    let idx = index;
    
    while (level.length > 1) {
      const siblingIdx = idx % 2 === 0 ? idx + 1 : idx - 1;
      proof.push(level[siblingIdx]);
      
      const next: Uint8Array[] = [];
      for (let i = 0; i < level.length; i += 2) {
        const combined = new Uint8Array(64);
        combined.set(level[i], 0);
        combined.set(level[i + 1], 32);
        next.push(blake2b(combined, { dkLen: 32 }));
      }
      level = next;
      idx = Math.floor(idx / 2);
    }
    
    return proof;
  }

  const cell0 = cells[0];
  const proof0 = generateProof(cells, 0);
  console.log("cell0 occupied:", cell0.isOccupied, "salt:", Array.from(cell0.salt).map(b => b.toString(16).padStart(2, '0')).join('').slice(0, 16) + "...");
  console.log("proof length:", proof0.length);

  // Manual SCALE encode
  const ScaleCell = Struct({ salt: Bytes(32), is_occupied: bool });
  const ScaleCoordinate = Struct({ x: u8, y: u8 });
  const ScaleCellReveal = Struct({
    cell: ScaleCell,
    proof: Vector(Bytes(32)),
    coord: ScaleCoordinate,
  });
  const ScaleRevealCellArgs = Struct({
    game_id: u64,
    reveal: ScaleCellReveal,
    expected_round: u32,
  });

  const argsEncoded = ScaleRevealCellArgs.enc({
    game_id: gameId,
    reveal: {
      cell: { salt: cell0.salt, is_occupied: cell0.isOccupied },
      proof: proof0,
      coord: { x: 0, y: 0 },
    },
    expected_round: 1,
  });
  const callData = new Uint8Array(2 + argsEncoded.length);
  callData[0] = PALLET;
  callData[1] = 0x04; // reveal_cell
  callData.set(argsEncoded, 2);

  const nRev = await getNonce(bobAddr);
  const wrappedBob = wrapSigner(bobSigner, callData);
  const dummyTx = api.tx.Battleship.surrender({ game_id: gameId });
  const sRev = await dummyTx.sign(wrappedBob, { mortality: { mortal: false }, nonce: nRev });
  const hRev = "0x" + Array.from(sRev as Uint8Array, (b: number) => b.toString(16).padStart(2, "0")).join("");
  const revHash = await rpc("author_submitExtrinsic", [hRev]);
  console.log("Reveal txHash:", revHash);
  
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 1000));
    const nn = await getNonce(bobAddr);
    if (nn > nRev) {
      console.log("Reveal included (nonce", nRev, "->", nn, ")");
      break;
    }
  }

  // Check game after reveal
  await new Promise(r => setTimeout(r, 3000));
  const g3Hex = await rpc("state_getStorage", [gKey]) as string | null;
  console.log("Game exists after reveal:", g3Hex !== null);

  // Check recent events for this game
  console.log("\n--- Checking events ---");
  // Get the block that included the reveal
  const latestHash = await rpc("chain_getBlockHash", []) as string;
  const blockBody = await rpc("chain_getBlock", [latestHash]) as any;
  const blockNum = parseInt(blockBody.block.header.number, 16);
  console.log("Latest block:", blockNum);
  
  // Check events at latest blocks
  for (let offset = 0; offset < 5; offset++) {
    const bHash = await rpc("chain_getBlockHash", [blockNum - offset]) as string;
    const eventsKey = "0x26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7"; // System.Events key
    const evHex = await rpc("state_getStorage", [eventsKey, bHash]) as string | null;
    if (evHex && evHex.includes("32")) { // Battleship pallet index in events
      console.log(`Block ${blockNum - offset}: has events (${evHex.length} chars)`);
    }
  }

  client.destroy();
  process.exit(0);
}

main().catch(e => { console.error(e); process.exit(1); });
