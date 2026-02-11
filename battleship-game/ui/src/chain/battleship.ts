import type { PolkadotClient, PolkadotSigner } from "polkadot-api";
import { AccountId } from "polkadot-api";
import { Bytes, Vector, Struct, u8, u32, u64, bool } from "scale-ts";
import { firstValueFrom } from "rxjs";
import type { ChainCell } from "../types/index.ts";

function bytesToHex(bytes: Uint8Array): string {
  return (
    "0x" +
    Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")
  );
}

// --- Manual SCALE encoding for calls with nested [u8;32] fields ---
// polkadot-api's fixedStr codec uses TextEncoder.encode() on hex strings,
// producing garbage for H256/[u8;32] fields in nested structs.
// We encode the call data manually using scale-ts with raw Uint8Array values.

const BATTLESHIP_PALLET_INDEX = 0x32; // 50
const COMMIT_GRID_CALL_INDEX = 0x02;
const REVEAL_CELL_CALL_INDEX = 0x04;
const REVEAL_WINNER_GRID_CALL_INDEX = 0x05;

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
const ScaleRevealWinnerGridArgs = Struct({
  game_id: u64,
  full_grid: Vector(ScaleCell),
});

function encodeCommitGridCall(
  gameId: bigint,
  gridRoot: Uint8Array,
): Uint8Array {
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

function encodeRevealWinnerGridCall(
  gameId: bigint,
  cells: ChainCell[],
): Uint8Array {
  const argsEncoded = ScaleRevealWinnerGridArgs.enc({
    game_id: gameId,
    full_grid: cells.map((c) => ({
      salt: c.salt,
      is_occupied: c.isOccupied,
    })),
  });
  const callData = new Uint8Array(2 + argsEncoded.length);
  callData[0] = BATTLESHIP_PALLET_INDEX;
  callData[1] = REVEAL_WINNER_GRID_CALL_INDEX;
  callData.set(argsEncoded, 2);
  return callData;
}

/** Wrap a signer so that signTx replaces the call data with our manually encoded bytes. */
function wrapSignerWithCallData(
  signer: PolkadotSigner,
  rawCallData: Uint8Array,
): PolkadotSigner {
  return {
    publicKey: signer.publicKey,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    signTx: ((_origCallData: Uint8Array, ...rest: any[]) =>
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (signer as any).signTx(rawCallData, ...rest)) as any,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    signBytes: (signer as any).signBytes,
  } as PolkadotSigner;
}

export interface TxResult {
  ok: boolean;
}

const TX_TIMEOUT_MS = 60000;

const localNonceMap = new Map<string, number>();

export function resetLocalNonce(address: string) {
  localNonceMap.delete(address);
}

async function submitFireAndForget(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  tx: any,
  signer: PolkadotSigner,
  client: PolkadotClient,
  label = "tx",
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  api?: any,
): Promise<TxResult> {
  const startTime = Date.now();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const pubkey = (signer as any).publicKey;
  const address = pubkey ? AccountId().dec(pubkey) : "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const request = (client as any)._request;

  // Query confirmed on-chain nonce from storage (not system_accountNextIndex which includes pool txs)
  let storageNonce = 0;
  try {
    if (api) {
      const acct = await api.query.System.Account.getValue(address, { at: "best" });
      storageNonce = acct?.nonce ?? 0;
    }
  } catch { /* ignore */ }
  const cachedNonce = localNonceMap.get(address) ?? 0;
  let nonce = Math.max(storageNonce, cachedNonce);

  console.log(`[${label}] nonce=${nonce} (t=${Date.now() - startTime}ms)`);

  // Smoldot parachain state can lag 1 block — hedge by trying nonce N and N+1
  let accepted = false;
  for (const tryNonce of [nonce, nonce + 1]) {
    try {
      const signedTx = await tx.sign(signer, { mortality: { mortal: false }, nonce: tryNonce });
      await request("author_submitExtrinsic", [bytesToHex(signedTx as Uint8Array)]);
      console.log(`[${label}] Accepted nonce=${tryNonce} (t=${Date.now() - startTime}ms)`);
      nonce = tryNonce;
      accepted = true;
      break;
    } catch (e) {
      const msg = String(e);
      if (msg.includes("1010") || msg.includes("Stale")) {
        console.log(`[${label}] Stale nonce=${tryNonce}, trying next`);
        continue;
      } else if (msg.includes("1014") || msg.includes("Priority") || msg.includes("Already")) {
        console.log(`[${label}] Already/Priority nonce=${tryNonce}, treating as accepted`);
        nonce = tryNonce;
        accepted = true;
        break;
      } else if (msg.includes("1012") || msg.includes("Future")) {
        console.log(`[${label}] Future nonce=${tryNonce}, previous was accepted`);
        nonce = tryNonce - 1;
        accepted = true;
        break;
      } else {
        console.error(`[${label}] Unexpected error nonce=${tryNonce}:`, msg.slice(0, 120));
        if (tryNonce === nonce) continue;
        throw e;
      }
    }
  }

  if (!accepted) {
    const lastNonce = nonce + 2;
    console.log(`[${label}] Both stale, last resort nonce=${lastNonce}`);
    const signedTx = await tx.sign(signer, { mortality: { mortal: false }, nonce: lastNonce });
    await request("author_submitExtrinsic", [bytesToHex(signedTx as Uint8Array)]);
    nonce = lastNonce;
  }

  localNonceMap.set(address, nonce + 1);
  console.log(`[${label}] Submitted nonce=${nonce} (t=${Date.now() - startTime}ms)`);
  return { ok: true };
}

export interface GameJoinedEvent {
  gameId: bigint;
  player2: string;
}

export interface AttackMadeEvent {
  gameId: bigint;
  attacker: string;
  coordinate: { x: number; y: number };
}

export interface AttackRevealedEvent {
  gameId: bigint;
  coordinate: { x: number; y: number };
  hit: boolean;
}

export interface GameEndedEvent {
  gameId: bigint;
  winner: string;
  loser: string;
  reason: string;
  prize: bigint;
}

export class BattleshipClient {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private api: any;
  private client: PolkadotClient;
  private gameEndedCache: Map<string, { winner: string; loser: string; reason: string }> = new Map();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  constructor(api: any, client: PolkadotClient) {
    this.api = api;
    this.client = client;
  }

  static async create(client: PolkadotClient): Promise<BattleshipClient> {
    return new BattleshipClient(client.getUnsafeApi(), client);
  }

  private async getNextGameId(): Promise<bigint> {
    try {
      const nextId = await this.api.query.Battleship.NextGameId.getValue({ at: "best" });
      if (nextId === undefined || nextId === null) return 0n;
      return typeof nextId === "bigint" ? nextId : BigInt(nextId);
    } catch {
      return 0n;
    }
  }

  async createGame(
    signer: PolkadotSigner,
    potAmount: bigint,
    maxRetries = 3
  ): Promise<{ ok: boolean; gameId?: bigint }> {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const pubkey = (signer as any).publicKey;
    const address = pubkey
      ? AccountId().dec(pubkey)
      : "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        const predictedId = await this.getNextGameId();
        console.log(`create_game(${attempt}): predicted gameId = ${predictedId}`);

        const tx = this.api.tx.Battleship.create_game({ pot_amount: potAmount });
        await submitFireAndForget(tx, signer, this.client, `create_game(${attempt})`, this.api);

        let confirmedId: bigint | undefined;
        for (let i = 0; i < 20; i++) {
          await new Promise(r => setTimeout(r, i < 4 ? 1500 : 500));
          const playerGame = await this.getPlayerGame(address);
          if (i < 5) console.log(`create_game: poll(${i}) =`, playerGame);
          if (playerGame !== null) {
            confirmedId = playerGame;
            break;
          }
        }

        if (confirmedId !== undefined) {
          console.log(`create_game: confirmed gameId=${confirmedId}`);
          return { ok: true, gameId: confirmedId };
        }

        console.log(`create_game(${attempt}): not confirmed, clearing nonce cache for retry`);
        localNonceMap.delete(address);
      } catch (e) {
        const msg = String(e);
        console.error(`create_game(${attempt}) failed:`, msg.slice(0, 100));
        localNonceMap.delete(address);
      }
      await new Promise(r => setTimeout(r, 3000));
    }
    return { ok: false };
  }

  async joinGame(
    signer: PolkadotSigner,
    gameId: bigint,
    maxRetries = 3
  ): Promise<{ ok: boolean }> {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const pubkey = (signer as any).publicKey;
    const address = pubkey ? AccountId().dec(pubkey) : "";

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        const tx = this.api.tx.Battleship.join_game({ game_id: gameId });
        await submitFireAndForget(tx, signer, this.client, `join_game(${attempt})`, this.api);

        for (let i = 0; i < 20; i++) {
          await new Promise(r => setTimeout(r, i < 4 ? 1500 : 500));
          const { game } = await this.getGame(gameId);
          if (game && game.phase?.type !== "WaitingForOpponent") {
            console.log(`join_game: confirmed, phase=${game.phase?.type}`);
            return { ok: true };
          }
          if (i < 5) console.log(`join_game: poll(${i}) phase=${game?.phase?.type}`);
        }

        console.log(`join_game(${attempt}): not confirmed, clearing nonce cache`);
        localNonceMap.delete(address);
      } catch (e) {
        console.error(`join_game(${attempt}) failed:`, String(e).slice(0, 100));
        localNonceMap.delete(address);
      }
      await new Promise(r => setTimeout(r, 3000));
    }
    return { ok: false };
  }

  async commitGrid(
    signer: PolkadotSigner,
    gameId: bigint,
    gridRoot: Uint8Array
  ): Promise<{ ok: boolean }> {
    try {
      const rawCallData = encodeCommitGridCall(gameId, gridRoot);
      const wrappedSigner = wrapSignerWithCallData(signer, rawCallData);
      // Use a dummy call to get a tx object, then sign with our wrapped signer
      // which injects the correctly-encoded call data
      const dummyTx = this.api.tx.Battleship.surrender({
        game_id: gameId,
      });
      await submitFireAndForget(dummyTx, wrappedSigner, this.client, "commit_grid", this.api);
      return { ok: true };
    } catch (e) {
      console.error("commit_grid error:", e);
      return { ok: false };
    }
  }

  async attack(
    signer: PolkadotSigner,
    gameId: bigint,
    x: number,
    y: number,
    round: number,
    _onReorged?: () => void
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.attack({
        game_id: gameId,
        coordinate: { x, y },
        expected_round: round,
      });
      await submitFireAndForget(tx, signer, this.client, "attack", this.api);
      return { ok: true };
    } catch (e) {
      console.error("attack error:", e);
      return { ok: false };
    }
  }

  async revealCell(
    signer: PolkadotSigner,
    gameId: bigint,
    cell: ChainCell,
    proof: Uint8Array[],
    coord: { x: number; y: number },
    round: number,
    _onReorged?: () => void,
  ): Promise<{ ok: boolean }> {
    try {
      const rawCallData = encodeRevealCellCall(
        gameId,
        cell,
        proof,
        coord,
        round,
      );
      const wrappedSigner = wrapSignerWithCallData(signer, rawCallData);
      // Use a dummy call to get a tx object, then sign with our wrapped signer
      // which injects the correctly-encoded call data
      const dummyTx = this.api.tx.Battleship.surrender({
        game_id: gameId,
      });
      await submitFireAndForget(dummyTx, wrappedSigner, this.client, "reveal_cell", this.api);
      return { ok: true };
    } catch (e) {
      console.error("reveal_cell error:", e);
      return { ok: false };
    }
  }

  async revealWinnerGrid(
    signer: PolkadotSigner,
    gameId: bigint,
    cells: ChainCell[],
  ): Promise<{ ok: boolean }> {
    try {
      const rawCallData = encodeRevealWinnerGridCall(gameId, cells);
      const wrappedSigner = wrapSignerWithCallData(signer, rawCallData);
      const dummyTx = this.api.tx.Battleship.surrender({
        game_id: gameId,
      });
      await submitFireAndForget(dummyTx, wrappedSigner, this.client, "reveal_winner_grid", this.api);
      return { ok: true };
    } catch (e) {
      console.error("reveal_winner_grid error:", e);
      return { ok: false };
    }
  }

  async surrender(
    signer: PolkadotSigner,
    gameId: bigint
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.surrender({ game_id: gameId });
      await submitFireAndForget(tx, signer, this.client, "surrender", this.api);
      return { ok: true };
    } catch (e) {
      console.error("surrender error:", e);
      return { ok: false };
    }
  }

  async cancelGame(
    signer: PolkadotSigner,
    gameId: bigint
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.cancel_game({ game_id: gameId });
      await submitFireAndForget(tx, signer, this.client, "cancel_game", this.api);
      return { ok: true };
    } catch (e) {
      console.error("cancel_game error:", e);
      return { ok: false };
    }
  }

  async claimTimeoutWin(
    signer: PolkadotSigner,
    gameId: bigint
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.claim_timeout_win({ game_id: gameId });
      await submitFireAndForget(tx, signer, this.client, "claim_timeout_win", this.api);
      return { ok: true };
    } catch (e) {
      console.error("claim_timeout_win error:", e);
      return { ok: false };
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async getGame(gameId: bigint, _atBlockHash?: string): Promise<{ game: any | null; error?: Error }> {
    try {
      const game = await this.api.query.Battleship.Games.getValue(gameId, {
        at: "best",
      });
      return { game };
    } catch (e) {
      console.error("getGame error:", e);
      return { game: null, error: e as Error };
    }
  }

  async gameExistsOnNode(gameId: bigint): Promise<boolean> {
    try {
      const game = await this.api.query.Battleship.Games.getValue(gameId, {
        at: "best",
      });
      return game !== null && game !== undefined;
    } catch {
      return true;
    }
  }

  async getBestBlockHash(): Promise<string> {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const block: any = await firstValueFrom(this.client.bestBlocks$ as any);
    return block?.hash;
  }

  // Query player data
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async getPlayerData(gameId: bigint, player: string): Promise<any | null> {
    try {
      const data = await this.api.query.Battleship.PlayerDataStorage.getValue(
        gameId,
        player,
        { at: "best" }
      );
      return data;
    } catch (e) {
      console.error("getPlayerData error:", e);
      return null;
    }
  }

  async getPlayerGame(player: string): Promise<bigint | null> {
    try {
      const gameId = await this.api.query.Battleship.PlayerGame.getValue(player, {
        at: "best",
      });
      if (gameId === null || gameId === undefined) return null;
      return typeof gameId === "bigint" ? gameId : BigInt(gameId);
    } catch (e) {
      console.error("[getPlayerGame] Failed:", e);
      return null;
    }
  }

  async findWaitingGames(): Promise<bigint[]> {
    try {
      const nextId = await this.getNextGameId();
      const waitingGames: bigint[] = [];
      const startId = nextId > 20n ? nextId - 20n : 0n;

      for (let id = startId; id < nextId; id++) {
        try {
          const game = await this.api.query.Battleship.Games.getValue(id, { at: "best" });
          if (game && game.phase && game.phase.type === "WaitingForOpponent") {
            waitingGames.push(id);
          }
        } catch { /* skip */ }
      }

      return waitingGames;
    } catch (e) {
      console.error("findWaitingGames error:", e);
      return [];
    }
  }

  async getGameEndedEvent(gameId: bigint): Promise<{ winner: string; loser: string; reason: string } | null> {
    const cacheKey = gameId.toString();
    const cached = this.gameEndedCache.get(cacheKey);
    if (cached) {
      console.log(`[getGameEndedEvent] Found cached event for game ${gameId}`);
      return cached;
    }

    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const request = (this.client as any)._request;
      const latestHash = await request("chain_getBlockHash", []) as string;
      const latestHeader = await request("chain_getHeader", [latestHash]) as { number: string };
      const latestNum = parseInt(latestHeader.number, 16);
      const SEARCH_DEPTH = 100;

      for (let n = latestNum; n >= Math.max(0, latestNum - SEARCH_DEPTH); n--) {
        try {
          const blockHash = await request("chain_getBlockHash", [n]) as string;
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const events: any[] = await this.api.query.System.Events.getValue({ at: blockHash });
          if (!events) continue;

          for (const event of events) {
            if (event.event?.type === "Battleship" && event.event?.value?.type === "GameEnded") {
              const data = event.event.value.value;
              if (data?.game_id === gameId) {
                const result = {
                  winner: data.winner?.toString() || "",
                  loser: data.loser?.toString() || "",
                  reason: data.reason?.type || "unknown",
                };
                console.log(`[getGameEndedEvent] Found in block #${n} (${blockHash})`);
                this.gameEndedCache.set(cacheKey, result);
                return result;
              }
            }
          }
        } catch { /* skip block */ }
      }
      return null;
    } catch (e) {
      console.error("getGameEndedEvent error:", e);
      return null;
    }
  }

  getCachedGameEndedEvent(gameId: bigint): { winner: string; loser: string; reason: string } | null {
    return this.gameEndedCache.get(gameId.toString()) ?? null;
  }

  cacheGameEndedEvent(gameId: bigint, event: { winner: string; loser: string; reason: string }): void {
    this.gameEndedCache.set(gameId.toString(), event);
  }

  subscribeToEvents(
    gameId: bigint,
    handlers: {
      onGameJoined?: (event: GameJoinedEvent) => void;
      onGridCommitted?: (player: string) => void;
      onGameStarted?: () => void;
      onAttackMade?: (event: AttackMadeEvent) => void;
      onAttackRevealed?: (event: AttackRevealedEvent) => void;
      onAllShipsSunk?: (pendingWinner: string) => void;
      onGameEnded?: (event: GameEndedEvent) => void;
    }
  ): () => void {
    console.log(`[subscribeToEvents] Starting subscription for game ${gameId}`);
    let cancelled = false;
    let gameEndedHandled = false;
    const processedHashes = new Set<string>();

    const processBlockByHash = async (blockHash: string) => {
      if (cancelled || gameEndedHandled) return;
      if (processedHashes.has(blockHash)) return;
      processedHashes.add(blockHash);

      try {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const events: any[] = await this.api.query.System.Events.getValue({ at: blockHash });
        if (!events) return;

        for (const event of events) {
          if (event.event?.type !== "Battleship") continue;

          const eventType = event.event?.value?.type;
          const data = event.event?.value?.value;

          if (data?.game_id !== gameId) continue;

          if (eventType === "GameEnded" && !gameEndedHandled) {
            gameEndedHandled = true;
            const endedEvent: GameEndedEvent = {
              gameId: data.game_id,
              winner: data.winner?.toString() || "",
              loser: data.loser?.toString() || "",
              reason: data.reason?.type || "unknown",
              prize: data.prize || 0n,
            };
            this.gameEndedCache.set(gameId.toString(), {
              winner: endedEvent.winner,
              loser: endedEvent.loser,
              reason: endedEvent.reason,
            });
            console.log(`[subscribeToEvents] GameEnded event in block ${blockHash} for game ${gameId}:`, endedEvent);
            handlers.onGameEnded?.(endedEvent);
          }
        }
      } catch (e) {
        console.error(`[subscribeToEvents] Error processing block ${blockHash}:`, e);
      }
    };

    const bestBlockSub = this.client.bestBlocks$.subscribe({
      next: (blocks: { hash: string }[]) => {
        for (const block of blocks) {
          processBlockByHash(block.hash);
        }
      },
      error: (err) => {
        console.error("[subscribeToEvents] Best block subscription error:", err);
      },
    });

    const finalizedSub = this.client.finalizedBlock$.subscribe({
      next: (block: { hash: string }) => {
        processBlockByHash(block.hash);
      },
      error: (err) => {
        console.error("[subscribeToEvents] Finalized subscription error:", err);
      },
    });

    const subscription = {
      unsubscribe: () => {
        bestBlockSub.unsubscribe();
        finalizedSub.unsubscribe();
      }
    };

    return () => {
      console.log(`[subscribeToEvents] Unsubscribing from game ${gameId}`);
      cancelled = true;
      subscription.unsubscribe();
    };
  }
}
