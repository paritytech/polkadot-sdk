import type { PolkadotClient, PolkadotSigner } from "polkadot-api";
import { Binary, type TxFinalizedPayload } from "polkadot-api";
import type { ChainCell } from "../types/index.ts";

export interface TxResult {
  result: TxFinalizedPayload;
  onReorged: (callback: () => void) => void;
}

const TX_TIMEOUT_MS = 60000;

async function submitAndWaitBestBlock(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  tx: any,
  signer: PolkadotSigner,
  label = "tx"
): Promise<TxResult> {
  const startTime = Date.now();
  console.log(`[${label}] Submitting transaction... (t=0ms)`);

  const observable = tx.signSubmitAndWatch(signer, {
    mortality: { mortal: true, period: 4096 },
  });

  let reorgCallback: (() => void) | null = null;

  return new Promise((resolve, reject) => {
    let resolved = false;

    const timeout = setTimeout(() => {
      if (!resolved) {
        sub.unsubscribe();
        console.log(`[${label}] Transaction timed out after ${TX_TIMEOUT_MS}ms`);
        reject(new Error(`Transaction timeout after ${TX_TIMEOUT_MS}ms`));
      }
    }, TX_TIMEOUT_MS);

    const sub = observable.subscribe({
      next: (e: { type: string; found?: boolean; block?: { number: number } }) => {
        console.log(`[${label}] Event: ${e.type}${e.found !== undefined ? ` found=${e.found}` : ''}${e.block ? ` block=${e.block.number}` : ''} (t=${Date.now() - startTime}ms)`);

        if (e.type === "finalized") {
          clearTimeout(timeout);
          sub.unsubscribe();
          return;
        }

        if (e.type === "invalid" || e.type === "dropped") {
          clearTimeout(timeout);
          sub.unsubscribe();
          if (!resolved) {
            reject(new Error(`Transaction ${e.type}`));
          } else {
            reorgCallback?.();
          }
        } else if (e.type === "txBestBlocksState") {
          if (e.found === true && !resolved) {
            clearTimeout(timeout);
            resolved = true;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const block = (e as any).block;
            console.log(`[${label}] Included in best block #${block?.number} (t=${Date.now() - startTime}ms)`);
            resolve({
              result: e as unknown as TxFinalizedPayload,
              onReorged: (cb) => { reorgCallback = cb; }
            });
          } else if (e.found === false && resolved) {
            console.log(`[${label}] Reorged out of best block! (t=${Date.now() - startTime}ms)`);
            reorgCallback?.();
          }
        }
      },
      error: (err: Error) => {
        clearTimeout(timeout);
        sub.unsubscribe();
        if (!resolved) {
          reject(err);
        } else {
          reorgCallback?.();
        }
      }
    });
  });
}

// NOTE: This file needs to be updated once PAPI descriptors are generated.
// For now, we use a dynamic API approach.
// After running `npx papi add battleship -w ws://localhost:9944 && npx papi generate`,
// update imports to use the generated types.

export interface GameCreatedEvent {
  gameId: bigint;
  player1: string;
  potAmount: bigint;
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

// Helper to extract events from transaction result
// Events have nested structure: { type: "PalletName", value: { type: "EventName", value: {...} } }
function extractEvents<T>(
  result: { events: Array<{ type: string; value: { type: string; value: unknown } }> },
  palletName: string,
  eventName: string
): T[] {
  return result.events
    .filter((e) => e.type === palletName && e.value?.type === eventName)
    .map((e) => e.value.value as T);
}

export class BattleshipClient {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private api: any;
  private client: PolkadotClient;
  // Cache for GameEnded events (gameId -> event)
  private gameEndedCache: Map<string, { winner: string; loser: string; reason: string }> = new Map();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  constructor(api: any, client: PolkadotClient) {
    this.api = api;
    this.client = client;
  }

  static async create(client: PolkadotClient): Promise<BattleshipClient> {
    return new BattleshipClient(client.getUnsafeApi(), client);
  }

  async createGame(
    signer: PolkadotSigner,
    potAmount: bigint
  ): Promise<{ ok: boolean; gameId?: bigint }> {
    try {
      const tx = this.api.tx.Battleship.create_game({
        pot_amount: potAmount,
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "create_game");

      if (!result.ok) {
        console.error("create_game failed:", JSON.stringify(result.dispatchError, null, 2));
        return { ok: false };
      }

      console.log("create_game events:", result.events);

      const events = extractEvents<{ game_id: bigint; player1: string; pot_amount: bigint }>(
        result,
        "Battleship",
        "GameCreated"
      );
      console.log("GameCreated events found:", events);

      if (events.length > 0) {
        return { ok: true, gameId: events[0].game_id };
      }

      return { ok: true };
    } catch (e) {
      console.error("create_game error:", e);
      return { ok: false };
    }
  }

  async joinGame(
    signer: PolkadotSigner,
    gameId: bigint
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.join_game({
        game_id: gameId,
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "join_game");
      return { ok: result.ok };
    } catch (e) {
      console.error("join_game error:", e);
      return { ok: false };
    }
  }

  async commitGrid(
    signer: PolkadotSigner,
    gameId: bigint,
    gridRoot: Uint8Array
  ): Promise<{ ok: boolean }> {
    try {
      const rootHex = Array.from(gridRoot).map(b => b.toString(16).padStart(2, '0')).join('');
      console.log("commit_grid:", { gameId, gridRootHex: rootHex });
      const tx = this.api.tx.Battleship.commit_grid({
        game_id: gameId,
        grid_root: Binary.fromBytes(gridRoot),
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "commit_grid");
      return { ok: result.ok };
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
    onReorged?: () => void
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.attack({
        game_id: gameId,
        coordinate: { x, y },
        expected_round: round,
      });
      const { result, onReorged: registerReorg } = await submitAndWaitBestBlock(tx, signer, "attack");
      if (onReorged) {
        registerReorg(onReorged);
      }
      return { ok: result.ok };
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
    onReorged?: () => void
  ): Promise<{ ok: boolean }> {
    try {
      const saltHex = Array.from(cell.salt).map(b => b.toString(16).padStart(2, '0')).join('');
      console.log("reveal_cell full data:", {
        gameId,
        saltHex,
        saltLength: cell.salt.length,
        isOccupied: cell.isOccupied,
        proofLength: proof.length,
        coord,
        round,
      });

      const tx = this.api.tx.Battleship.reveal_cell({
        game_id: gameId,
        reveal: {
          cell: {
            salt: Binary.fromBytes(cell.salt),
            is_occupied: cell.isOccupied,
          },
          proof: proof.map((p) => Binary.fromBytes(p)),
          coord: { x: coord.x, y: coord.y },
        },
        expected_round: round,
      });
      const { result, onReorged: registerReorg } = await submitAndWaitBestBlock(tx, signer, "reveal_cell");
      
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const events = (result as any).events;
      if (events) {
        for (const event of events) {
          if (event.event?.type === "Battleship" && event.event?.value?.type === "GameEnded") {
            const data = event.event.value.value;
            console.error("reveal_cell caused GameEnded:", {
              winner: data.winner?.toString(),
              loser: data.loser?.toString(),
              reason: data.reason?.type,
            });
          }
        }
      }
      
      if (!result.ok) {
        console.error("reveal_cell dispatch error:", result.dispatchError);
      }
      if (onReorged) {
        registerReorg(onReorged);
      }
      return { ok: result.ok };
    } catch (e) {
      console.error("reveal_cell error:", e);
      return { ok: false };
    }
  }

  async revealWinnerGrid(
    signer: PolkadotSigner,
    gameId: bigint,
    cells: ChainCell[]
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.reveal_winner_grid({
        game_id: gameId,
        full_grid: cells.map((c) => ({
          salt: Binary.fromBytes(c.salt),
          is_occupied: c.isOccupied,
        })),
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "reveal_winner_grid");
      return { ok: result.ok };
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
      const tx = this.api.tx.Battleship.surrender({
        game_id: gameId,
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "surrender");
      if (!result.ok) {
        console.error("surrender dispatch error:", JSON.stringify(result.dispatchError, null, 2));
      }
      return { ok: result.ok };
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
      const tx = this.api.tx.Battleship.cancel_game({
        game_id: gameId,
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "cancel_game");
      if (!result.ok) {
        console.error("cancel_game dispatch error:", JSON.stringify(result.dispatchError, null, 2));
      }
      return { ok: result.ok };
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
      const tx = this.api.tx.Battleship.claim_timeout_win({
        game_id: gameId,
      });
      const { result } = await submitAndWaitBestBlock(tx, signer, "claim_timeout_win");
      return { ok: result.ok };
    } catch (e) {
      console.error("claim_timeout_win error:", e);
      return { ok: false };
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async getGame(gameId: bigint, atBlockHash?: string): Promise<{ game: any | null; error?: Error }> {
    try {
      const game = await this.api.query.Battleship.Games.getValue(gameId, { at: atBlockHash || "best" });
      return { game };
    } catch (e) {
      console.error("getGame error:", e);
      return { game: null, error: e as Error };
    }
  }

  async getBestBlockHash(): Promise<string> {
    const { first } = await import('rxjs');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const block = await this.client.bestBlocks$.pipe(first()).toPromise() as any;
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
      const gameId = await this.api.query.Battleship.PlayerGame.getValue(player, { at: "best" });
      return gameId ?? null;
    } catch (e) {
      console.error("[getPlayerGame] Failed:", e);
      return null;
    }
  }

  async findWaitingGames(): Promise<bigint[]> {
    try {
      const entries = await this.api.query.Battleship.Games.getEntries({ at: "best" });
      const waitingGames: bigint[] = [];

      for (const entry of entries) {
        const game = entry.value;
        if (game && game.phase && game.phase.type === "WaitingForOpponent") {
          waitingGames.push(entry.keyArgs[0] as bigint);
        }
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
      for (const at of ["best", "finalized"] as const) {
        const events = await this.api.query.System.Events.getValue({ at });
        
        for (const event of events) {
          if (event.event?.type === "Battleship" && event.event?.value?.type === "GameEnded") {
            const data = event.event.value.value;
            if (data?.game_id === gameId) {
              const result = {
                winner: data.winner?.toString() || "",
                loser: data.loser?.toString() || "",
                reason: data.reason?.type || "unknown",
              };
              this.gameEndedCache.set(cacheKey, result);
              return result;
            }
          }
        }
      }
      return null;
    } catch (e) {
      console.error("getGameEndedEvent error:", e);
      return null;
    }
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

    const processBlockEvents = async (at: "best" | "finalized") => {
      if (cancelled || gameEndedHandled) return;
      
      try {
        const events = await this.api.query.System.Events.getValue({ at });
        
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
            console.log(`[subscribeToEvents] GameEnded event found in ${at} block for game ${gameId}:`, endedEvent);
            handlers.onGameEnded?.(endedEvent);
          }
        }
      } catch (e) {
        console.error(`[subscribeToEvents] Error processing ${at} block:`, e);
      }
    };

    const bestBlockSub = this.client.bestBlocks$.subscribe({
      next: () => {
        processBlockEvents("best");
      },
      error: (err) => {
        console.error("[subscribeToEvents] Best block subscription error:", err);
      },
    });

    const finalizedSub = this.client.finalizedBlock$.subscribe({
      next: () => {
        processBlockEvents("finalized");
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
