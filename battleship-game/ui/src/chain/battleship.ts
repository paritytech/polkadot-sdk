import type { PolkadotClient, PolkadotSigner } from "polkadot-api";
import { Binary, type TxFinalizedPayload } from "polkadot-api";
import type { ChainCell } from "../types/index.ts";
import { firstValueFrom, filter, tap } from "rxjs";

// Helper to submit tx and wait for best block inclusion (NOT finalization)
async function submitAndWaitBestBlock(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  tx: any,
  signer: PolkadotSigner,
  label = "tx"
): Promise<TxFinalizedPayload> {
  const startTime = Date.now();
  console.log(`[${label}] Submitting transaction... (t=0ms)`);
  const observable = tx.signSubmitAndWatch(signer, {
    mortality: { mortal: true, period: 256 },
  });

  // Wait for txBestBlocksState with found=true (included in best block, not finalized)
  const result = await firstValueFrom(
    observable.pipe(
      tap((e: { type: string; found?: boolean; block?: { number: number } }) =>
        console.log(`[${label}] Event: ${e.type}${e.found !== undefined ? ` found=${e.found}` : ''}${e.block ? ` block=${e.block.number}` : ''} (t=${Date.now() - startTime}ms)`)
      ),
      filter((e: { type: string; found?: boolean }) =>
        e.type === "txBestBlocksState" && e.found === true
      )
    )
  );
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const block = (result as any).block;
  console.log(`[${label}] Included in best block #${block?.number} (t=${Date.now() - startTime}ms)`);
  return result as TxFinalizedPayload;
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

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  constructor(api: any) {
    this.api = api;
  }

  static async create(client: PolkadotClient): Promise<BattleshipClient> {
    // Use unsafe API until descriptors are generated
    const api = client.getUnsafeApi();
    return new BattleshipClient(api);
  }

  async createGame(
    signer: PolkadotSigner,
    potAmount: bigint
  ): Promise<{ ok: boolean; gameId?: bigint }> {
    try {
      const tx = this.api.tx.Battleship.create_game({
        pot_amount: potAmount,
      });
      const result = await submitAndWaitBestBlock(tx, signer, "create_game");

      if (!result.ok) {
        console.error("create_game failed:", result.dispatchError);
        return { ok: false };
      }

      // Log all events to see structure
      console.log("create_game events:", result.events);

      // Extract GameCreated event
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
      const result = await submitAndWaitBestBlock(tx, signer, "join_game");
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
      const tx = this.api.tx.Battleship.commit_grid({
        game_id: gameId,
        grid_root: Binary.fromBytes(gridRoot),
      });
      const result = await submitAndWaitBestBlock(tx, signer, "commit_grid");
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
    y: number
  ): Promise<{ ok: boolean }> {
    try {
      const tx = this.api.tx.Battleship.attack({
        game_id: gameId,
        coordinate: { x, y },
      });
      const result = await submitAndWaitBestBlock(tx, signer, "attack");
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
    proof: Uint8Array[]
  ): Promise<{ ok: boolean }> {
    try {
      console.log("reveal_cell params:", {
        gameId,
        cell: { salt: Array.from(cell.salt).slice(0, 8), isOccupied: cell.isOccupied },
        proofLength: proof.length,
      });

      const tx = this.api.tx.Battleship.reveal_cell({
        game_id: gameId,
        reveal: {
          cell: {
            salt: Binary.fromBytes(cell.salt),
            is_occupied: cell.isOccupied,
          },
          proof: proof.map((p) => Binary.fromBytes(p)),
        },
      });
      const result = await submitAndWaitBestBlock(tx, signer, "reveal_cell");
      if (!result.ok) {
        console.error("reveal_cell dispatch error:", result.dispatchError);
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
      const result = await submitAndWaitBestBlock(tx, signer, "reveal_winner_grid");
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
      const result = await submitAndWaitBestBlock(tx, signer, "surrender");
      if (!result.ok) {
        console.error("surrender dispatch error:", JSON.stringify(result.dispatchError, null, 2));
      }
      return { ok: result.ok };
    } catch (e) {
      console.error("surrender error:", e);
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
      const result = await submitAndWaitBestBlock(tx, signer, "claim_timeout_win");
      return { ok: result.ok };
    } catch (e) {
      console.error("claim_timeout_win error:", e);
      return { ok: false };
    }
  }

  // Query game state
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async getGame(gameId: bigint): Promise<any | null> {
    try {
      const game = await this.api.query.Battleship.Games.getValue(gameId, { at: "best" });
      return game;
    } catch (e) {
      console.error("getGame error:", e);
      return null;
    }
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

  // Query player's current game
  async getPlayerGame(player: string): Promise<bigint | null> {
    try {
      const gameId = await this.api.query.Battleship.PlayerGame.getValue(player, { at: "best" });
      return gameId;
    } catch (e) {
      console.error("getPlayerGame error:", e);
      return null;
    }
  }

  // Find games waiting for opponent
  async findWaitingGames(): Promise<bigint[]> {
    try {
      // Query all games and filter by phase
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

  // Subscribe to finalized blocks for events
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
    // This is a simplified version - in production you'd use proper event subscriptions
    // For now, we'll rely on polling in the game state machine
    console.log("Event subscription setup for game:", gameId, handlers);
    return () => {
      console.log("Event subscription cleanup for game:", gameId);
    };
  }
}
