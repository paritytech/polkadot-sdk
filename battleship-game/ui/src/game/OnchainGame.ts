import { Board } from "./Board.ts";
import { getShipCells } from "./Ship.ts";
import { getChainClient, disconnectClient } from "../chain/client.ts";
import {
  getDevPlayerAccount,
  getOpponentAddress,
  type PlayerAccount,
} from "../chain/accounts.ts";
import {
  buildMerkleTree,
  generateProof,
  createChainCells,
  coordToIndex,
  type MerkleTree,
} from "../chain/merkle.ts";
import { BattleshipClient } from "../chain/battleship.ts";
import type {
  Player,
  ChainCell,
  OnchainPhase,
  Position,
  Orientation,
  ShipDefinition,
} from "../types/index.ts";

export interface OnchainGameState {
  phase: OnchainPhase;
  gameId: bigint | null;
  player: Player;
  isOurTurn: boolean;
  pendingAttack: Position | null;
  message: string;
  ourHits: number;
  opponentHits: number;
  winner: Player | null;
}

type StateChangeCallback = (state: OnchainGameState) => void;
type MessageCallback = (message: string) => void;
type GameEndCallback = (winner: Player, reason: string) => void;

export class OnchainGame {
  private player: Player;
  private account: PlayerAccount;

  private ourBoard: Board;
  private opponentBoard: Board;

  private gameId: bigint | null = null;
  private battleshipClient: BattleshipClient | null = null;

  private ourCells: ChainCell[] = [];
  private merkleTree: MerkleTree | null = null;

  private phase: OnchainPhase = "menu";
  private isOurTurn = false;
  private pendingAttack: Position | null = null;
  private ourHits = 0;
  private opponentHits = 0;
  private winner: Player | null = null;
  private message = "";

  private onStateChangeCallback: StateChangeCallback | null = null;
  private onMessageCallback: MessageCallback | null = null;
  private onGameEndCallback: GameEndCallback | null = null;

  private pollInterval: number | null = null;
  private lastLoggedPhase: string | null = null;
  private lastOpponentDataHash: string = "";

  private currentShipIndex = 0;
  private placementOrientation: Orientation = "horizontal";

  constructor(player: Player, account?: PlayerAccount) {
    this.player = player;
    this.account = account ?? getDevPlayerAccount(player as "alice" | "bob");
    this.ourBoard = new Board();
    this.opponentBoard = new Board();
  }

  async initialize(): Promise<void> {
    const client = await getChainClient();
    this.battleshipClient = await BattleshipClient.create(client);
    this.setMessage(`Connected as ${this.player.toUpperCase()}`);
  }

  // State management
  getState(): OnchainGameState {
    return {
      phase: this.phase,
      gameId: this.gameId,
      player: this.player,
      isOurTurn: this.isOurTurn,
      pendingAttack: this.pendingAttack,
      message: this.message,
      ourHits: this.ourHits,
      opponentHits: this.opponentHits,
      winner: this.winner,
    };
  }

  onStateChange(callback: StateChangeCallback): void {
    this.onStateChangeCallback = callback;
  }

  onMessageChange(callback: MessageCallback): void {
    this.onMessageCallback = callback;
  }

  onGameEnd(callback: GameEndCallback): void {
    this.onGameEndCallback = callback;
  }

  private notifyStateChange(): void {
    this.onStateChangeCallback?.(this.getState());
  }

  private setMessage(msg: string): void {
    this.message = msg;
    this.onMessageCallback?.(msg);
    this.notifyStateChange();
  }

  private setPhase(phase: OnchainPhase): void {
    this.phase = phase;
    this.notifyStateChange();
  }

  // Board accessors
  getOurBoard(): Board {
    return this.ourBoard;
  }

  getOpponentBoard(): Board {
    return this.opponentBoard;
  }

  // Ship placement
  getCurrentShipIndex(): number {
    return this.currentShipIndex;
  }

  getPlacementOrientation(): Orientation {
    return this.placementOrientation;
  }

  toggleOrientation(): void {
    this.placementOrientation =
      this.placementOrientation === "horizontal" ? "vertical" : "horizontal";
    this.notifyStateChange();
  }

  getCurrentShip(): ShipDefinition | null {
    const SHIPS: ShipDefinition[] = [
      { id: "carrier", name: "Carrier", size: 5 },
      { id: "battleship", name: "Battleship", size: 4 },
      { id: "cruiser", name: "Cruiser", size: 3 },
      { id: "submarine", name: "Submarine", size: 3 },
      { id: "destroyer", name: "Destroyer", size: 2 },
    ];
    if (this.currentShipIndex >= SHIPS.length) return null;
    return SHIPS[this.currentShipIndex];
  }

  canPlaceCurrentShip(pos: Position): boolean {
    const ship = this.getCurrentShip();
    if (!ship) return false;
    return this.ourBoard.canPlace(ship, pos, this.placementOrientation);
  }

  placeShip(pos: Position): boolean {
    const ship = this.getCurrentShip();
    if (!ship) return false;

    if (this.ourBoard.placeShip(ship, pos, this.placementOrientation)) {
      this.currentShipIndex++;
      this.notifyStateChange();
      return true;
    }
    return false;
  }

  placeShipsRandomly(): void {
    this.ourBoard.placeShipsRandomly();
    this.currentShipIndex = 5;
    this.notifyStateChange();
  }

  canStartBattle(): boolean {
    return this.ourBoard.allShipsPlaced();
  }

  canAttack(): boolean {
    return (
      this.phase === "battle" &&
      this.isOurTurn &&
      this.pendingAttack === null &&
      !this.isAttacking &&
      !this.isRevealing
    );
  }

  // Check if player is already in a game and resume it
  async checkAndResumeGame(): Promise<boolean> {
    if (!this.battleshipClient) return false;

    this.setMessage("Checking for existing game...");

    const existingGameId = await this.battleshipClient.getPlayerGame(
      this.account.address
    );

    console.log(`[${this.player}] getPlayerGame result:`, existingGameId, typeof existingGameId);

    if (existingGameId !== null && existingGameId !== undefined) {
      this.gameId = existingGameId;
      this.setMessage(`Resuming game #${this.gameId}...`);
      // Immediately fetch game state to set correct phase
      await this.pollGameState();
      this.startPolling();
      return true;
    }

    console.log(`[${this.player}] No existing game found, will create new one`);
    return false;
  }

  async createGame(potAmount: bigint): Promise<boolean> {
    if (!this.battleshipClient) return false;

    if (await this.checkAndResumeGame()) {
      return true;
    }

    this.setPhase("creating");
    this.setMessage("Creating game...");

    console.log(`[${this.player}] Creating new game with pot ${potAmount}...`);
    const result = await this.battleshipClient.createGame(
      this.account.signer,
      potAmount
    );

    console.log(`[${this.player}] Create game result:`, result);

    if (result.ok) {
      if (result.gameId !== undefined) {
        this.gameId = result.gameId;
      } else {
        console.log(`[${this.player}] GameId not in events, querying...`);
        const queriedGameId = await this.battleshipClient.getPlayerGame(this.account.address);
        if (queriedGameId !== null && queriedGameId !== undefined) {
          this.gameId = queriedGameId;
          console.log(`[${this.player}] Found gameId from query:`, this.gameId);
        }
      }

      if (this.gameId !== null) {
        this.setPhase("waiting_opponent");
        this.setMessage(`Game #${this.gameId} created. Waiting for opponent...`);
        this.startPolling();
        return true;
      }
    }

    this.setPhase("menu");
    this.setMessage("Failed to create game - check console for details");
    return false;
  }

  async joinGame(): Promise<boolean> {
    if (!this.battleshipClient) return false;

    if (await this.checkAndResumeGame()) {
      return true;
    }

    this.setMessage("Looking for games...");

    const waitingGames = await this.battleshipClient.findWaitingGames();

    if (waitingGames.length === 0) {
      this.setMessage("No games available to join");
      return false;
    }

    const gameId = waitingGames[0];
    this.setMessage(`Joining game #${gameId}...`);

    const result = await this.battleshipClient.joinGame(
      this.account.signer,
      gameId
    );

    if (result.ok) {
      this.gameId = gameId;
      this.setPhase("setup");
      this.setMessage("Joined! Place your ships.");
      this.startPolling();
      return true;
    }

    this.setMessage("Failed to join game");
    return false;
  }

  async joinExistingGame(gameId: bigint): Promise<boolean> {
    if (!this.battleshipClient) return false;

    if (await this.checkAndResumeGame()) {
      return true;
    }

    this.setMessage(`Joining game #${gameId}...`);

    const result = await this.battleshipClient.joinGame(
      this.account.signer,
      gameId
    );

    if (result.ok) {
      this.gameId = gameId;
      this.setPhase("setup");
      this.setMessage("Joined! Place your ships.");
      this.startPolling();
      return true;
    }

    this.setMessage("Failed to join game");
    return false;
  }

  // Commit grid to chain
  async commitGrid(): Promise<boolean> {
    if (!this.battleshipClient || this.gameId === null) return false;
    if (!this.ourBoard.allShipsPlaced()) return false;

    this.setMessage("Committing grid...");

    // Build chain cells from our board
    const occupiedIndices = new Set<number>();
    for (const ship of this.ourBoard.getShips()) {
      const cells = getShipCells(ship);
      for (const cell of cells) {
        occupiedIndices.add(coordToIndex(cell.x, cell.y));
      }
    }

    this.ourCells = createChainCells(occupiedIndices);
    this.merkleTree = buildMerkleTree(this.ourCells);

    console.log(`[${this.player}] Merkle root:`, Array.from(this.merkleTree.root).map(b => b.toString(16).padStart(2, '0')).join(''));
    console.log(`[${this.player}] Occupied indices:`, Array.from(occupiedIndices));

    const result = await this.battleshipClient.commitGrid(
      this.account.signer,
      this.gameId,
      this.merkleTree.root
    );

    if (result.ok) {
      this.setPhase("waiting_commit");
      this.setMessage("Grid committed. Waiting for opponent...");
      // Immediately poll to check if opponent already committed
      await this.pollGameState();
      return true;
    }

    this.setMessage("Failed to commit grid");
    return false;
  }

  // Attack opponent
  private isAttacking = false;

  async attack(pos: Position): Promise<boolean> {
    if (!this.battleshipClient || this.gameId === null) return false;
    if (!this.isOurTurn) return false;
    if (this.pendingAttack !== null) return false; // Can't attack during pending reveal
    if (this.isAttacking) return false; // Prevent double-click

    this.isAttacking = true;
    console.log(`[${this.player}] Attacking ${String.fromCharCode(65 + pos.x)}${pos.y + 1}...`);
    this.setMessage(`Attacking ${String.fromCharCode(65 + pos.x)}${pos.y + 1}...`);

    try {
      const result = await this.battleshipClient.attack(
        this.account.signer,
        this.gameId,
        pos.x,
        pos.y
      );

      console.log(`[${this.player}] Attack result:`, result);

      if (result.ok) {
        // Optimistically set local state - the chain has the pending attack now
        this.pendingAttack = pos;
        this.setMessage("Attack sent. Waiting for opponent to reveal...");
        this.notifyStateChange();
        return true;
      }

      this.setMessage("Attack failed");
      return false;
    } finally {
      this.isAttacking = false;
    }
  }

  // Handle incoming attack (auto-reveal)
  private isRevealing = false;

  private async handlePendingAttack(coord: Position): Promise<void> {
    if (!this.battleshipClient || this.gameId === null) return;
    if (this.isRevealing) return; // Prevent concurrent reveals

    if (!this.merkleTree) {
      console.error(`[${this.player}] Cannot reveal - no merkle tree! Did you resume a game without placing ships?`);
      this.setMessage("ERROR: Cannot reveal - local state lost. Please surrender.");
      return;
    }

    this.isRevealing = true;
    const index = coordToIndex(coord.x, coord.y);
    const cell = this.ourCells[index];
    const proof = generateProof(this.merkleTree, index);

    console.log(`[${this.player}] Revealing cell ${String.fromCharCode(65 + coord.x)}${coord.y + 1} (index ${index})...`);
    console.log(`[${this.player}] Cell:`, { isOccupied: cell.isOccupied, saltPrefix: Array.from(cell.salt.slice(0, 4)) });
    console.log(`[${this.player}] Proof length:`, proof.length);

    // Verify proof locally before sending
    const { verifyProof, getCellLeafHash } = await import("../chain/merkle.ts");
    const leafHash = getCellLeafHash(cell);
    const localValid = verifyProof(this.merkleTree.root, proof, 100, index, leafHash);
    console.log(`[${this.player}] Local proof verification:`, localValid);

    this.setMessage(`Revealing cell ${String.fromCharCode(65 + coord.x)}${coord.y + 1}...`);

    try {
      const result = await this.battleshipClient.revealCell(
        this.account.signer,
        this.gameId,
        cell,
        proof
      );

      console.log(`[${this.player}] Reveal result:`, result);

      if (result.ok) {
        // Update our local board to show the hit/miss
        if (cell.isOccupied) {
          this.ourBoard.receiveAttack(coord);
        }
        this.setMessage("Cell revealed.");
      } else {
        this.setMessage("Failed to reveal cell");
      }
    } finally {
      this.isRevealing = false;
      this.pendingAttack = null; // Always reset to allow retry on next poll
    }
  }

  // Surrender
  async surrender(): Promise<boolean> {
    if (!this.battleshipClient || this.gameId === null) {
      console.error(`[${this.player}] Cannot surrender - no client or gameId`);
      return false;
    }

    console.log(`[${this.player}] Sending surrender for game #${this.gameId}...`);
    this.setMessage("Surrendering...");

    const result = await this.battleshipClient.surrender(
      this.account.signer,
      this.gameId
    );

    console.log(`[${this.player}] Surrender result:`, result);

    if (result.ok) {
      this.winner = this.player === "alice" ? "bob" : "alice";
      this.gameId = null; // Clear game ID so we don't try to resume
      this.setPhase("finished");
      this.setMessage("You surrendered.");
      this.stopPolling();
      this.onGameEndCallback?.(this.winner, "surrender");
      return true;
    }

    this.setMessage("Surrender failed");
    return false;
  }

  // Polling for game state updates
  private isPolling = false;

  private startPolling(): void {
    if (this.pollInterval) return;

    this.pollInterval = window.setInterval(async () => {
      // Prevent overlapping polls
      if (this.isPolling) return;
      this.isPolling = true;
      try {
        await this.pollGameState();
      } finally {
        this.isPolling = false;
      }
    }, 1000); // Poll every 1 second for faster response
  }

  private stopPolling(): void {
    if (this.pollInterval) {
      clearInterval(this.pollInterval);
      this.pollInterval = null;
    }
  }

  private async pollGameState(): Promise<void> {
    if (!this.battleshipClient || this.gameId === null) return;

    const game = await this.battleshipClient.getGame(this.gameId);
    if (!game) {
      // Game no longer exists - opponent may have surrendered or game ended
      console.log(`[${this.player}] Game #${this.gameId} no longer exists`);
      if (this.phase !== "finished" && this.phase !== "menu") {
        this.winner = this.player; // We won if game disappeared while we were playing
        this.setPhase("finished");
        this.setMessage("Game ended - opponent may have left.");
        this.stopPolling();
        this.onGameEndCallback?.(this.player, "opponent_left");
      }
      return;
    }

    // Process phase changes
    await this.processGameState(game);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private async processGameState(game: any): Promise<void> {
    const phase = game.phase;
    const phaseKey = JSON.stringify(phase);
    if (phaseKey !== this.lastLoggedPhase) {
      console.log(`[${this.player}] Phase changed: ${phase?.type}`, phase?.value);
      this.lastLoggedPhase = phaseKey;
    }

    // Check for timeout (720 blocks = ~3 minutes at 4 blocks/second)
    const TURN_TIMEOUT = 720;
    const lastActionBlock = game.last_action_block;
    if (lastActionBlock) {
      // Check timeout during battle, setup (waiting for opponent commit), or waiting for opponent
      const canClaimTimeout =
        this.phase === "battle" ||
        this.phase === "waiting_commit" ||
        this.phase === "waiting_opponent";
      if (canClaimTimeout) {
        await this.checkAndClaimTimeout(lastActionBlock, TURN_TIMEOUT, phase);
      }
    }

    if (phase.type === "WaitingForOpponent") {
      if (this.phase !== "waiting_opponent") {
        this.setPhase("waiting_opponent");
        this.setMessage(`Game #${this.gameId} - Waiting for opponent...`);
      }
      return;
    }

    if (phase.type === "Setup") {
      // Check if both players committed
      const p1Ready = phase.value?.player1_ready ?? false;
      const p2Ready = phase.value?.player2_ready ?? false;
      console.log(`[${this.player}] Setup phase - p1Ready: ${p1Ready}, p2Ready: ${p2Ready}`);

      if (this.phase !== "setup" && this.phase !== "waiting_commit") {
        this.setPhase("setup");
        this.setMessage("Place your ships.");
      }

      if ((this.player === "alice" && p1Ready) || (this.player === "bob" && p2Ready)) {
        if (this.phase !== "waiting_commit") {
          this.setPhase("waiting_commit");
          this.setMessage("Waiting for opponent to commit grid...");
        }
      }
      return;
    }

    if (phase.type === "Playing") {
      if (this.phase !== "battle") {
        this.setPhase("battle");
      }

      // Update opponent board with revealed attack results
      await this.updateOpponentBoardFromChain();

      const currentTurn = phase.value?.current_turn;
      const pendingAttack = phase.value?.pending_attack;

      // Determine if it's our turn
      const weArePlayer1 = this.player === "alice";
      const isPlayer1Turn = currentTurn?.type === "Player1";
      this.isOurTurn = weArePlayer1 === isPlayer1Turn;

      console.log(`[${this.player}] current_turn: ${currentTurn?.type}, pendingAttack:`, pendingAttack, `isOurTurn: ${this.isOurTurn}, isRevealing: ${this.isRevealing}, isAttacking: ${this.isAttacking}`);

      if (pendingAttack) {
        // There's a pending attack - store it locally
        const coord = { x: pendingAttack.x, y: pendingAttack.y };
        this.pendingAttack = coord;

        // The defender (NOT current_turn) needs to reveal
        // For UI purposes, neither player can attack during pending reveal
        if (!this.isOurTurn && !this.isRevealing) {
          // We're the defender, reveal the cell
          console.log(`[${this.player}] Need to reveal cell at`, coord);
          await this.handlePendingAttack(coord);
        } else if (this.isOurTurn) {
          // We're the attacker, waiting for reveal
          this.setMessage("Waiting for opponent to reveal...");
        }
      } else {
        // Only clear pendingAttack if we're not the attacker waiting for reveal
        // (chain might not have updated yet)
        if (!this.isOurTurn) {
          this.pendingAttack = null;
        }

        // Don't update message if we're in the middle of attacking or waiting for reveal
        if (this.isAttacking) {
          // Keep current "Attacking..." message
        } else if (this.pendingAttack !== null) {
          // We attacked but chain hasn't updated yet - keep waiting message
          this.setMessage("Waiting for opponent to reveal...");
        } else if (this.isOurTurn) {
          this.setMessage("Your turn - click enemy waters to attack!");
        } else {
          this.setMessage("Opponent's turn...");
        }
      }

      this.notifyStateChange();
      return;
    }

    if (phase.type === "PendingWinnerReveal") {
      const winnerRole = phase.value?.winner;
      const weArePlayer1 = this.player === "alice";
      const weAreWinner =
        (winnerRole?.type === "Player1" && weArePlayer1) ||
        (winnerRole?.type === "Player2" && !weArePlayer1);

      if (weAreWinner) {
        // We need to reveal our grid
        this.setPhase("revealing");
        this.setMessage("You won! Revealing your grid...");
        await this.revealWinnerGrid();
      } else {
        this.setMessage("Opponent sunk all ships. Waiting for verification...");
      }
      return;
    }

    if (phase.type === "Finished") {
      const winnerRole = phase.value?.winner;
      const reason = phase.value?.reason?.type || "unknown";
      const weArePlayer1 = this.player === "alice";
      const weWon =
        (winnerRole?.type === "Player1" && weArePlayer1) ||
        (winnerRole?.type === "Player2" && !weArePlayer1);

      this.winner = weWon ? this.player : (this.player === "alice" ? "bob" : "alice");
      this.setPhase("finished");

      // Set appropriate message based on reason
      if (reason === "Surrender") {
        this.setMessage(weWon ? "Opponent surrendered! You win!" : "You surrendered.");
      } else if (reason === "Timeout") {
        this.setMessage(weWon ? "Opponent timed out! You win!" : "You timed out.");
      } else {
        this.setMessage(weWon ? "Victory!" : "Defeat!");
      }

      this.stopPolling();
      this.onGameEndCallback?.(this.winner, reason.toLowerCase());
      return;
    }
  }

  // Check if opponent has timed out and auto-claim win
  private isClaimingTimeout = false;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private async checkAndClaimTimeout(lastActionBlock: number, timeout: number, phase: any): Promise<void> {
    if (!this.battleshipClient || this.gameId === null) return;
    if (this.isClaimingTimeout) return;

    // Determine if we can claim timeout based on phase
    // - WaitingForOpponent: player1 can cancel and get refund
    // - Setup: if opponent hasn't committed, we can claim
    // - Playing: if it's opponent's turn (not ours), we can claim
    let canClaim = false;
    if (phase.type === "WaitingForOpponent") {
      canClaim = true; // Creator can cancel
    } else if (phase.type === "Setup") {
      // Can claim if we already committed but opponent hasn't
      const weArePlayer1 = this.player === "alice";
      const p1Ready = phase.value?.player1_ready ?? false;
      const p2Ready = phase.value?.player2_ready ?? false;
      canClaim = (weArePlayer1 && p1Ready && !p2Ready) || (!weArePlayer1 && p2Ready && !p1Ready);
    } else if (phase.type === "Playing") {
      canClaim = !this.isOurTurn; // Only claim if it's opponent's turn
    }

    if (!canClaim) return;

    try {
      // Get current block from chain
      const client = await getChainClient();
      const api = client.getUnsafeApi();
      const currentBlock = await api.query.System.Number.getValue({ at: "best" });

      const timeoutBlock = lastActionBlock + timeout;
      if (currentBlock >= timeoutBlock) {
        console.log(`[${this.player}] Opponent timed out! Block ${currentBlock} >= ${timeoutBlock}`);
        this.isClaimingTimeout = true;
        this.setMessage("Opponent timed out! Claiming win...");

        const result = await this.battleshipClient.claimTimeoutWin(
          this.account.signer,
          this.gameId
        );

        console.log(`[${this.player}] Claim timeout result:`, result);
        this.isClaimingTimeout = false;
      }
    } catch (e) {
      console.error(`[${this.player}] Error checking timeout:`, e);
      this.isClaimingTimeout = false;
    }
  }

  // Update opponent board based on chain data (our attacks on opponent)
  private async updateOpponentBoardFromChain(): Promise<void> {
    if (!this.battleshipClient || this.gameId === null) return;

    const opponentAddress = getOpponentAddress(this.player);
    const opponentData = await this.battleshipClient.getPlayerData(this.gameId, opponentAddress);
    if (!opponentData) return;

    // Skip if no change
    const dataHash = JSON.stringify(opponentData);
    if (dataHash === this.lastOpponentDataHash) return;
    this.lastOpponentDataHash = dataHash;

    console.log(`[${this.player}] Opponent data:`, opponentData);

    // hit_cells contains coordinates where opponent was hit (our successful attacks)
    const hitCells = opponentData.hit_cells || [];
    const hitSet = new Set(hitCells.map((c: { x: number; y: number }) => `${c.x},${c.y}`));

    // revealed is an array of 100 elements where revealed[i] === 1 means cell i was attacked
    const revealed = opponentData.revealed;
    if (!revealed || !Array.isArray(revealed)) return;

    for (let i = 0; i < 100 && i < revealed.length; i++) {
      if (revealed[i] === 1) {
        const x = i % 10;
        const y = Math.floor(i / 10);
        const isHit = hitSet.has(`${x},${y}`);
        this.opponentBoard.markAttackResult({ x, y }, isHit);
      }
    }
  }

  private async revealWinnerGrid(): Promise<void> {
    if (!this.battleshipClient || this.gameId === null) return;

    const result = await this.battleshipClient.revealWinnerGrid(
      this.account.signer,
      this.gameId,
      this.ourCells
    );

    if (result.ok) {
      this.setMessage("Grid revealed successfully!");
    } else {
      this.setMessage("Failed to reveal grid");
    }
  }

  // Cleanup
  destroy(): void {
    this.stopPolling();
    disconnectClient();
  }

  // Reset for new game
  reset(): void {
    this.ourBoard.reset();
    this.opponentBoard.reset();
    this.ourCells = [];
    this.merkleTree = null;
    this.gameId = null;
    this.currentShipIndex = 0;
    this.placementOrientation = "horizontal";
    this.isOurTurn = false;
    this.pendingAttack = null;
    this.ourHits = 0;
    this.opponentHits = 0;
    this.winner = null;
    this.lastLoggedPhase = null;
    this.lastOpponentDataHash = "";
    this.setPhase("menu");
    this.setMessage("");
    this.stopPolling();
  }
}
