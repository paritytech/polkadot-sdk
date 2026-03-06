import { BattleshipClient } from "./battleship.js";
import { BotAccount } from "./accounts.js";
import { placeShipsRandomly, selectAttackTarget } from "./game.js";
import { buildMerkleTree } from "./merkle.js";
import { GRID_SIZE } from "./types.js";
import type { Position } from "./types.js";

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
  amIPlayer1: boolean;
  notFoundCount: number;
}

export class BattleshipBot {
  private client: BattleshipClient;
  private account: BotAccount;
  private games: Map<bigint, GameState> = new Map();

  constructor(client: BattleshipClient, account: BotAccount) {
    this.client = client;
    this.account = account;
  }

  async run(): Promise<void> {
    console.log(`[Bot] Starting with address ${this.account.address}`);

    while (true) {
      try {
        // Only look for new games if we're not in any
        if (this.games.size === 0) {
          await this.findOrCreateGame();
        }

        // Play all active games
        await this.playActiveGames();

        // Wait before next iteration
        await new Promise(r => setTimeout(r, 3000));
      } catch (e) {
        console.error("[Bot] Error in main loop:", e);
        await new Promise(r => setTimeout(r, 5000));
      }
    }
  }

  private async findOrCreateGame(): Promise<void> {
    // Look for games to join (that aren't ours)
    const waitingGames = await this.client.findWaitingGames();
    
    for (const gameId of waitingGames) {
      const game = await this.client.getGame(gameId);
      if (!game) continue;
      
      const player1 = game.player1?.toString();
      // Skip games we created
      if (player1 === this.account.address) continue;
      
      // Try to join this game
      console.log(`[Bot] Joining game ${gameId}...`);
      const success = await this.client.joinGame(this.account.signer, gameId);
      
      if (success) {
        await this.initializeGame(gameId);
        return; // Successfully joined, done
      }
    }
    
    // No games to join, create one
    console.log("[Bot] Creating new game...");
    const gameId = await this.client.createGame(this.account.signer, 1000000000000n);
    
    if (gameId !== null) {
      await this.initializeGame(gameId);
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

    // Determine if it's my turn
    const myTurnType = state.amIPlayer1 ? "Player1" : "Player2";
    const isMyTurn = currentTurn === myTurnType;

    // If there's a pending attack and I'm the defender (NOT current_turn), I must reveal
    if (pendingAttack && !isMyTurn) {
      const { x, y } = pendingAttack;
      const coordKey = `${x},${y}`;
      
      // Skip if we just revealed this exact coordinate (prevent duplicate reveals)
      if (state.lastRevealCoord === coordKey) {
        return;
      }
      
      console.log(`[Bot] Need to reveal (${x}, ${y}) in game ${gameId}, round ${currentRound}`);
      await this.revealAttackedCell(gameId, state, { x, y }, currentRound);
      state.lastRevealCoord = coordKey;
      return;
    }

    // Clear last reveal coord if no pending attack
    if (!pendingAttack) {
      state.lastRevealCoord = "";
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
