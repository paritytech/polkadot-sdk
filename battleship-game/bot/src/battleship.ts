import type { PolkadotClient, PolkadotSigner } from "polkadot-api";
import { AccountId } from "polkadot-api";
import { Bytes, Vector, Struct, u8, u32, u64, bool } from "scale-ts";
import type { ChainCell } from "./types.js";

function bytesToHex(bytes: Uint8Array): string {
  return "0x" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

const BATTLESHIP_PALLET_INDEX = 0x32;
const COMMIT_GRID_CALL_INDEX = 0x02;
const REVEAL_CELL_CALL_INDEX = 0x04;

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
const ScaleCommitGridArgs = Struct({
  game_id: u64,
  grid_root: Bytes(32),
});

function encodeCommitGridCall(gameId: bigint, gridRoot: Uint8Array): Uint8Array {
  const argsEncoded = ScaleCommitGridArgs.enc({
    game_id: gameId,
    grid_root: gridRoot,
  });
  const callData = new Uint8Array(2 + argsEncoded.length);
  callData[0] = BATTLESHIP_PALLET_INDEX;
  callData[1] = COMMIT_GRID_CALL_INDEX;
  callData.set(argsEncoded, 2);
  return callData;
}

function encodeRevealCellCall(
  gameId: bigint,
  cell: ChainCell,
  proof: Uint8Array[],
  coord: { x: number; y: number },
  expectedRound: number,
): Uint8Array {
  const argsEncoded = ScaleRevealCellArgs.enc({
    game_id: gameId,
    reveal: {
      cell: { salt: cell.salt, is_occupied: cell.isOccupied },
      proof,
      coord,
    },
    expected_round: expectedRound,
  });
  const callData = new Uint8Array(2 + argsEncoded.length);
  callData[0] = BATTLESHIP_PALLET_INDEX;
  callData[1] = REVEAL_CELL_CALL_INDEX;
  callData.set(argsEncoded, 2);
  return callData;
}

function wrapSignerWithCallData(
  signer: PolkadotSigner,
  rawCallData: Uint8Array,
): PolkadotSigner {
  return {
    publicKey: signer.publicKey,
    signTx: ((_origCallData: Uint8Array, ...rest: any[]) =>
      (signer as any).signTx(rawCallData, ...rest)) as any,
    signBytes: (signer as any).signBytes,
  } as PolkadotSigner;
}

const localNonceMap = new Map<string, number>();

async function submitTx(
  tx: any,
  signer: PolkadotSigner,
  client: PolkadotClient,
  label: string,
  api?: any,
): Promise<boolean> {
  const pubkey = (signer as any).publicKey;
  const address = AccountId().dec(pubkey);
  const request = (client as any)._request;

  let storageNonce = 0;
  try {
    if (api) {
      const acct = await api.query.System.Account.getValue(address, { at: "best" });
      storageNonce = acct?.nonce ?? 0;
    }
  } catch { }
  
  const cachedNonce = localNonceMap.get(address) ?? 0;
  let nonce = Math.max(storageNonce, cachedNonce);

  let accepted = false;
  for (const tryNonce of [nonce, nonce + 1]) {
    try {
      const signedTx = await tx.sign(signer, { mortality: { mortal: false }, nonce: tryNonce });
      await request("author_submitExtrinsic", [bytesToHex(signedTx as Uint8Array)]);
      console.log(`[${label}] Accepted nonce=${tryNonce}`);
      nonce = tryNonce;
      accepted = true;
      break;
    } catch (e) {
      const msg = String(e);
      if (msg.includes("1010") || msg.includes("Stale")) {
        continue;
      } else if (msg.includes("1014") || msg.includes("Priority") || msg.includes("Already")) {
        nonce = tryNonce;
        accepted = true;
        break;
      } else if (msg.includes("1012") || msg.includes("Future")) {
        nonce = tryNonce - 1;
        accepted = true;
        break;
      } else {
        console.error(`[${label}] Error nonce=${tryNonce}:`, msg.slice(0, 120));
        if (tryNonce === nonce) continue;
        throw e;
      }
    }
  }

  if (!accepted) {
    const lastNonce = nonce + 2;
    const signedTx = await tx.sign(signer, { mortality: { mortal: false }, nonce: lastNonce });
    await request("author_submitExtrinsic", [bytesToHex(signedTx as Uint8Array)]);
    nonce = lastNonce;
  }

  localNonceMap.set(address, nonce + 1);
  return true;
}

export class BattleshipClient {
  private api: any;
  private client: PolkadotClient;

  constructor(api: any, client: PolkadotClient) {
    this.api = api;
    this.client = client;
  }

  static async create(client: PolkadotClient): Promise<BattleshipClient> {
    return new BattleshipClient(client.getUnsafeApi(), client);
  }

  async createGame(signer: PolkadotSigner, potAmount: bigint): Promise<bigint | null> {
    try {
      const nextId = await this.api.query.Battleship.NextGameId.getValue({ at: "best" });
      const predictedId = typeof nextId === "bigint" ? nextId : BigInt(nextId);
      
      // Get current player game BEFORE creating (to detect new game)
      const pubkey = (signer as any).publicKey;
      const address = AccountId().dec(pubkey);
      const oldPlayerGame = await this.api.query.Battleship.PlayerGame.getValue(address, { at: "best" });
      const oldGameId = oldPlayerGame ? (typeof oldPlayerGame === "bigint" ? oldPlayerGame : BigInt(oldPlayerGame)) : null;
      
      const tx = this.api.tx.Battleship.create_game({ pot_amount: potAmount });
      await submitTx(tx, signer, this.client, "create_game", this.api);
      
      // Poll for NEW game (different from old)
      for (let i = 0; i < 20; i++) {
        await new Promise(r => setTimeout(r, 1500));
        const playerGame = await this.api.query.Battleship.PlayerGame.getValue(address, { at: "best" });
        if (playerGame !== null && playerGame !== undefined) {
          const gameId = typeof playerGame === "bigint" ? playerGame : BigInt(playerGame);
          // Only return if it's a NEW game or no old game existed
          if (oldGameId === null || gameId !== oldGameId) {
            console.log(`[create_game] Confirmed NEW gameId=${gameId}`);
            return gameId;
          }
        }
      }
      
      // Fallback: assume the predicted ID if NextGameId increased
      const newNextId = await this.api.query.Battleship.NextGameId.getValue({ at: "best" });
      const newNext = typeof newNextId === "bigint" ? newNextId : BigInt(newNextId);
      console.log(`[create_game] Fallback check: newNext=${newNext}, predictedId=${predictedId}`);
      if (newNext > predictedId) {
        console.log(`[create_game] Using predicted gameId=${predictedId}`);
        return predictedId;
      }
      
      console.log(`[create_game] Failed: NextGameId did not increase`);
      return null;
    } catch (e) {
      console.error("[create_game] Error:", e);
      return null;
    }
  }


  async joinGame(signer: PolkadotSigner, gameId: bigint): Promise<boolean> {
    try {
      const tx = this.api.tx.Battleship.join_game({ game_id: gameId });
      await submitTx(tx, signer, this.client, "join_game", this.api);
      
      // Poll for confirmation
      for (let i = 0; i < 20; i++) {
        await new Promise(r => setTimeout(r, 1500));
        const game = await this.api.query.Battleship.Games.getValue(gameId, { at: "best" });
        if (game && game.phase?.type !== "WaitingForOpponent") {
          console.log(`[join_game] Confirmed, phase=${game.phase?.type}`);
          return true;
        }
      }
      return false;
    } catch (e) {
      console.error("[join_game] Error:", e);
      return false;
    }
  }

  async commitGrid(signer: PolkadotSigner, gameId: bigint, gridRoot: Uint8Array): Promise<boolean> {
    try {
      const rawCallData = encodeCommitGridCall(gameId, gridRoot);
      const wrappedSigner = wrapSignerWithCallData(signer, rawCallData);
      const dummyTx = this.api.tx.Battleship.surrender({ game_id: gameId });
      await submitTx(dummyTx, wrappedSigner, this.client, "commit_grid", this.api);
      return true;
    } catch (e) {
      console.error("[commit_grid] Error:", e);
      return false;
    }
  }

  async attack(signer: PolkadotSigner, gameId: bigint, x: number, y: number, round: number): Promise<boolean> {
    try {
      const tx = this.api.tx.Battleship.attack({
        game_id: gameId,
        coordinate: { x, y },
        expected_round: round,
      });
      await submitTx(tx, signer, this.client, "attack", this.api);
      return true;
    } catch (e) {
      console.error("[attack] Error:", e);
      return false;
    }
  }

  async revealCell(
    signer: PolkadotSigner,
    gameId: bigint,
    cell: ChainCell,
    proof: Uint8Array[],
    coord: { x: number; y: number },
    round: number,
  ): Promise<boolean> {
    try {
      const rawCallData = encodeRevealCellCall(gameId, cell, proof, coord, round);
      const wrappedSigner = wrapSignerWithCallData(signer, rawCallData);
      const dummyTx = this.api.tx.Battleship.surrender({ game_id: gameId });
      await submitTx(dummyTx, wrappedSigner, this.client, "reveal_cell", this.api);
      return true;
    } catch (e) {
      console.error("[reveal_cell] Error:", e);
      return false;
    }
  }

  async surrender(signer: PolkadotSigner, gameId: bigint): Promise<boolean> {
    try {
      const tx = this.api.tx.Battleship.surrender({ game_id: gameId });
      await submitTx(tx, signer, this.client, "surrender", this.api);
      return true;
    } catch (e) {
      console.error("[surrender] Error:", e);
      return false;
    }
  }

  async getGame(gameId: bigint): Promise<any> {
    try {
      return await this.api.query.Battleship.Games.getValue(gameId, { at: "best" });
    } catch (e) {
      console.error("[getGame] Error:", e);
      return null;
    }
  }

  async getPlayerData(gameId: bigint, player: string): Promise<any> {
    try {
      return await this.api.query.Battleship.PlayerDataStorage.getValue(gameId, player, { at: "best" });
    } catch (e) {
      console.error("[getPlayerData] Error:", e);
      return null;
    }
  }

  async getPlayerGame(address: string): Promise<bigint | null> {
    try {
      const val = await this.api.query.Battleship.PlayerGame.getValue(address, { at: "best" });
      if (val === null || val === undefined) return null;
      return typeof val === "bigint" ? val : BigInt(val);
    } catch {
      return null;
    }
  }

  async findWaitingGames(): Promise<bigint[]> {
    try {
      const nextId = await this.api.query.Battleship.NextGameId.getValue({ at: "best" });
      const next = typeof nextId === "bigint" ? nextId : BigInt(nextId);
      const waitingGames: bigint[] = [];
      const startId = next > 20n ? next - 20n : 0n;

      for (let id = startId; id < next; id++) {
        try {
          const game = await this.api.query.Battleship.Games.getValue(id, { at: "best" });
          if (game && game.phase && game.phase.type === "WaitingForOpponent") {
            waitingGames.push(id);
          }
        } catch { }
      }

      return waitingGames;
    } catch (e) {
      console.error("[findWaitingGames] Error:", e);
      return [];
    }
  }
}
