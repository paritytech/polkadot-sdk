import { BattleshipClient, resetLocalNonce } from "./battleship.js";
import { BotAccount } from "./accounts.js";
import { placeShipsRandomly, selectAttackTarget } from "./game.js";
import { buildMerkleTree } from "./merkle.js";
import { subscribeToBestBlocks } from "./client.js";
import { GRID_SIZE } from "./types.js";
import type { Position } from "./types.js";
import type { StatementStoreClient, GameAnnouncement, JoinRequest } from "./statementStore.js";

interface GameState {
  gameId: bigint;
  myAddress: string;
  opponentAddress: string;
  myGrid: any[];
  myRoot: Uint8Array;
  opponentHits: Set<string>;
  myAttacks: Map<string, boolean>;
  round: number;
  committed: boolean;
  lastAttackRound: number;
  lastRevealCoord: string;
  lastRevealTime: number;
  amIPlayer1: boolean;
  notFoundCount: number;
}

export class BattleshipBot {
  private client: BattleshipClient;
  private account: BotAccount;
  private games: Map<bigint, GameState> = new Map();
  private statementStore: StatementStoreClient | null = null;
  private announcementTimestamp: number | null = null;
  private waitingForJoin = false;
  private processingJoinRequest = false;
  private botName: string;
  private lastAnnounceTime = 0;

  constructor(client: BattleshipClient, account: BotAccount, statementStore?: StatementStoreClient) {
    this.client = client;
    this.account = account;
    this.statementStore = statementStore ?? null;
    this.botName = `Bot-${account.address.slice(0, 6)}`;
    this.setupStatementHandlers();
  }

  private setupStatementHandlers(): void {
    if (!this.statementStore) return;

    // Respond to liveness pings while we have an announced game
    this.statementStore.onPing(async (ping) => {
      if (ping.creator !== this.account.address) return;
      if (!this.statementStore || !this.waitingForJoin) return;
      console.log(`[Bot] Received ping from ${ping.pinger.slice(0, 8)}..., sending pong`);
      await this.statementStore.sendLivenessPong(ping, this.account.publicKey, this.account.rawSign);
    });

    // Handle join requests: create game on-chain and notify joiner
    this.statementStore.onJoinRequest(async (req: JoinRequest) => {
      if (req.creator !== this.account.address) return;
      if (req.gameTimestamp !== this.announcementTimestamp) return;
      if (!this.waitingForJoin || this.processingJoinRequest) return;
      if (this.games.size > 0) return;

      this.processingJoinRequest = true;
      try {
        console.log(`[Bot] Received join request from ${req.joiner.slice(0, 8)}...`);

        // NOW create the game on-chain
        console.log("[Bot] Creating game on-chain...");
        const gameId = await this.client.createGame(this.account.signer, 1000000000000n);
        if (gameId === null) {
          console.error("[Bot] Failed to create game on-chain");
          return;
        }

        // Notify the joiner with the on-chain game ID
        await this.statementStore!.sendGameCreated(
          this.account.address,
          this.announcementTimestamp!,
          req.joiner,
          gameId.toString(),
          this.account.publicKey,
          this.account.rawSign,
        );
        console.log(`[Bot] Game ${gameId} created on-chain, notified ${req.joiner.slice(0, 8)}...`);

        await this.initializeGame(gameId);
        this.waitingForJoin = false;
      } finally {
        this.processingJoinRequest = false;
      }
    });
  }

  async run(): Promise<void> {
    console.log(`[Bot] Starting as "${this.botName}" with address ${this.account.address}`);
    let pendingTick = true;
    let wakeLoop: (() => void) | null = null;

    const requestTick = () => {
      pendingTick = true;
      if (wakeLoop) {
        const wake = wakeLoop;
        wakeLoop = null;
        wake();
      }
    };

    const unsubscribe = await subscribeToBestBlocks(requestTick);

    try {
      while (true) {
        if (!pendingTick) {
          await new Promise<void>((resolve) => {
            wakeLoop = resolve;
          });
        }
        pendingTick = false;

        try {
          if (this.games.size === 0) {
            await this.findOrCreateGame();
          }

          await this.playActiveGames();
        } catch (e) {
          console.error("[Bot] Error in main loop:", e);
        }
      }
    } finally {
      unsubscribe();
    }
  }

  private async findOrCreateGame(): Promise<void> {
    // Check if we're already in a game on-chain (resume after restart)
    const existingGameId = await this.client.getPlayerGame(this.account.address);
    if (existingGameId !== null) {
      const game = await this.client.getGame(existingGameId);
      if (game) {
        const phase = game.phase?.type;
        if (phase === "Finished") {
          console.log(`[Bot] Found finished game ${existingGameId}, surrendering to clean up...`);
          await this.client.surrender(this.account.signer, existingGameId);
          await new Promise(r => setTimeout(r, 6000));
          return;
        }
        console.log(`[Bot] Found existing on-chain game ${existingGameId} (phase=${phase}), resuming...`);
        await this.initializeGame(existingGameId);
        return;
      } else {
        console.log(`[Bot] PlayerGame points to missing game ${existingGameId}, surrendering to clean up...`);
        await this.client.surrender(this.account.signer, existingGameId);
        await new Promise(r => setTimeout(r, 6000));
        return;
      }
    }

    // Re-announce periodically so new UI clients can discover us
    const now = Date.now();
    const shouldAnnounce = !this.waitingForJoin || (now - this.lastAnnounceTime > 30_000);
    if (!shouldAnnounce) return;

    // Announce game intent via statement store (NO on-chain game yet)
    if (this.statementStore) {
      if (!this.waitingForJoin) {
        this.announcementTimestamp = Date.now();
      }
      const announcement: GameAnnouncement = {
        creator: this.account.address,
        creatorName: this.botName,
        potAmount: "1000000000000",
        timestamp: this.announcementTimestamp!,
      };
      await this.statementStore.announceGame(announcement, this.account.publicKey, this.account.rawSign);
      this.lastAnnounceTime = now;
      this.waitingForJoin = true;
      console.log("[Bot] Game announced via statement store, waiting for opponent...");
    }
  }

  private async initializeGame(gameId: bigint): Promise<void> {
    if (this.games.has(gameId)) {
      return; // Already initialized
    }

    const game = await this.client.getGame(gameId);
    if (!game) return;

    const player1 = game.player1?.toString();
    const player2 = game.player2?.toString();
    const myAddress = this.account.address;
    
    // Verify we're in the game
    if (player1 !== myAddress && player2 !== myAddress) {
      console.log(`[Bot] Not in game ${gameId}`);
      return;
    }
    
    const opponentAddress = player1 === myAddress ? player2 : player1;
    const amIPlayer1 = player1 === myAddress;

    // Place ships and generate merkle tree
    const myGrid = placeShipsRandomly();
    const { root } = buildMerkleTree(myGrid);

    // Store game state
    const state: GameState = {
      gameId,
      myAddress,
      opponentAddress: opponentAddress || "",
      myGrid,
      myRoot: root,
      opponentHits: new Set(),
      myAttacks: new Map(),
      round: 0,
      committed: false,
      lastAttackRound: -1,
      lastRevealCoord: "",
      lastRevealTime: 0,
      amIPlayer1,
      notFoundCount: 0,
    };
    
    this.games.set(gameId, state);
    console.log(`[Bot] Initialized game ${gameId}, I am Player${amIPlayer1 ? '1' : '2'}, opponent=${opponentAddress || "waiting"}`);
  }

  private async playActiveGames(): Promise<void> {
    for (const [gameId, state] of this.games.entries()) {
      try {
        await this.playGame(gameId, state);
      } catch (e) {
        console.error(`[Bot] Error in game ${gameId}:`, e);
      }
    }
  }

  private async playGame(gameId: bigint, state: GameState): Promise<void> {
    const game = await this.client.getGame(gameId);
    if (!game) {
      // Don't immediately remove - could be temporary
      state.notFoundCount++;
      if (state.notFoundCount >= 3) {
        console.log(`[Bot] Game ${gameId} not found after ${state.notFoundCount} attempts, removing`);
        this.games.delete(gameId);
      }
      return;
    }
    
    // Reset not found counter
    state.notFoundCount = 0;

    const phase = game.phase?.type;

    // Handle finished games
    if (phase === "Finished") {
      console.log(`[Bot] Game ${gameId} finished`);
      this.games.delete(gameId);
      return;
    }

    // Handle PendingWinnerReveal — we need to reveal our full grid
    if (phase === "PendingWinnerReveal") {
      const winnerRole = game.phase?.value?.winner?.type;
      const weAreWinner = (winnerRole === "Player1" && state.amIPlayer1) ||
                          (winnerRole === "Player2" && !state.amIPlayer1);
      if (weAreWinner) {
        console.log(`[Bot] We won game ${gameId}! Revealing full grid...`);
        resetLocalNonce(state.myAddress);
        const success = await this.client.revealWinnerGrid(
          this.account.signer,
          gameId,
          state.myGrid,
        );
        if (success) {
          console.log(`[Bot] Winner grid revealed for game ${gameId}`);
        } else {
          console.log(`[Bot] Failed to reveal winner grid, will retry`);
        }
      } else {
        console.log(`[Bot] Opponent won game ${gameId}, waiting for their grid reveal...`);
      }
      return;
    }

    // Update opponent address if it was empty
    if (!state.opponentAddress) {
      const player1 = game.player1?.toString();
      const player2 = game.player2?.toString();
      const opponentAddress = player1 === state.myAddress ? player2 : player1;
      
      if (opponentAddress) {
        state.opponentAddress = opponentAddress;
        console.log(`[Bot] Game ${gameId} opponent joined: ${opponentAddress}`);
      }
    }

    // Commit grid if we haven't yet and opponent has joined
    if (!state.committed && state.opponentAddress && phase === "Setup") {
      console.log(`[Bot] Committing grid for game ${gameId}...`);
      const success = await this.client.commitGrid(this.account.signer, gameId, state.myRoot);
      
      if (success) {
        state.committed = true;
        console.log(`[Bot] Grid committed for game ${gameId}`);
      }
      return;
    }

    // Wait for playing phase
    if (phase !== "Playing") {
      return;
    }

    // Get the playing phase data
    const playingPhase = game.phase.value;
    if (!playingPhase) {
      return;
    }

    const currentRound = playingPhase.round ? Number(playingPhase.round) : 0;
    const currentTurn = playingPhase.current_turn?.type; // "Player1" or "Player2"
    const pendingAttack = playingPhase.pending_attack;

    // Detect fork: round went backwards or state doesn't match our tracking
    if (currentRound < state.lastAttackRound) {
      console.log(`[Bot] Fork detected in game ${gameId}: round ${state.lastAttackRound} -> ${currentRound}, resetting state`);
      state.lastAttackRound = -1;
      state.lastRevealCoord = "";
      resetLocalNonce(state.myAddress);
    }

    // Determine if it's my turn
    const myTurnType = state.amIPlayer1 ? "Player1" : "Player2";
    const isMyTurn = currentTurn === myTurnType;

    // If there's a pending attack and I'm the defender (NOT current_turn), I must reveal
    if (pendingAttack && !isMyTurn) {
      const { x, y } = pendingAttack;
      const coordKey = `${x},${y}`;
      
      // Skip if we recently revealed this exact coordinate (prevent rapid duplicate reveals)
      // But retry after 15s in case the tx went stale
      if (state.lastRevealCoord === coordKey) {
        const elapsed = Date.now() - state.lastRevealTime;
        if (elapsed < 15_000) {
          return;
        }
        console.log(`[Bot] Retrying stale reveal for (${x}, ${y}) after ${(elapsed / 1000).toFixed(0)}s`);
        resetLocalNonce(state.myAddress);
      }

      console.log(`[Bot] Need to reveal (${x}, ${y}) in game ${gameId}, round ${currentRound}`);
      await this.revealAttackedCell(gameId, state, { x, y }, currentRound);
      state.lastRevealCoord = coordKey;
      state.lastRevealTime = Date.now();
      return;
    }

    // Clear last reveal coord if no pending attack
    if (!pendingAttack) {
      state.lastRevealCoord = "";
      state.lastRevealTime = 0;
    }

    // If no pending attack and it's my turn, I should attack
    if (!pendingAttack && isMyTurn && state.lastAttackRound < currentRound) {
      console.log(`[Bot] My turn to attack in game ${gameId}, round ${currentRound}`);
      await this.makeAttack(gameId, state, currentRound);
      return;
    }
  }

  private async makeAttack(gameId: bigint, state: GameState, round: number): Promise<void> {
    const target = selectAttackTarget(state.myAttacks);
    
    console.log(`[Bot] Attacking (${target.x}, ${target.y}) in game ${gameId}, round ${round}`);
    
    const success = await this.client.attack(
      this.account.signer,
      gameId,
      target.x,
      target.y,
      round
    );

    if (success) {
      state.lastAttackRound = round;
      state.myAttacks.set(`${target.x},${target.y}`, false);
      console.log(`[Bot] Attack submitted for round ${round}`);
    }
  }

  private async revealAttackedCell(
    gameId: bigint,
    state: GameState,
    coordinate: { x: number; y: number },
    round: number
  ): Promise<void> {
    const { x, y } = coordinate;
    const index = y * GRID_SIZE + x;
    const cell = state.myGrid[index];

    const isHit = cell.isOccupied;
    console.log(`[Bot] Revealing (${x}, ${y}): ${isHit ? 'HIT ○' : 'MISS ×'} in game ${gameId}, round ${round}`);

    const { proofs } = buildMerkleTree(state.myGrid);
    const proof = proofs[index];

    const success = await this.client.revealCell(
      this.account.signer,
      gameId,
      cell,
      proof,
      coordinate,
      round
    );

    if (success) {
      console.log(`[Bot] Cell revealed successfully`);
      if (cell.isOccupied) {
        state.opponentHits.add(`${x},${y}`);
      }
    }
  }
}
