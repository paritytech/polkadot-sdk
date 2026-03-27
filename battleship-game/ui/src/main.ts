import { OnchainGame } from "./game/OnchainGame.ts";
import { Renderer } from "./render/Renderer.ts";
import { InputHandler } from "./input/InputHandler.ts";
import { getOrCreateWallet, type PlayerAccount } from "./chain/accounts.ts";
import { getWalletManager, WalletManager } from "./chain/wallet.ts";
import { getChainClient } from "./chain/client.ts";
import { BattleshipClient, resetLocalNonce } from "./chain/battleship.ts";
import { getStatementChain } from "./chain/client.ts";
import { getStatementStore, type StatementStoreClient, type GameAnnouncement, type GameCreatedNotification } from "./chain/statementStore.ts";
import type { Player, Position } from "./types/index.ts";

type Screen = "username-screen" | "loading" | "game-lobby" | "game";

class BattleshipApp {
  private game: OnchainGame | null = null;
  private renderer: Renderer | null = null;
  private playerCanvas: HTMLCanvasElement | null = null;
  private enemyCanvas: HTMLCanvasElement | null = null;
  private playerCtx: CanvasRenderingContext2D | null = null;
  private enemyCtx: CanvasRenderingContext2D | null = null;
  private playerInput: InputHandler | null = null;
  private enemyInput: InputHandler | null = null;

  private playerHover: Position | null = null;
  private enemyHover: Position | null = null;
  private lastTime = 0;

  private walletManager: WalletManager;
  private currentAccount: PlayerAccount | null = null;
  private currentBalance: bigint = 0n;
  private selectedPotAmount: bigint = 0n;
  private lobbyRefreshInterval: number | null = null;
  private statementStore: StatementStoreClient | null = null;
  private playedGameKeys: Set<string> = new Set();
  private buttonAbortController: AbortController | null = null;
  private username = "";
  private opponentName = "";
  private activeGameCreator = "";
  private activeGameTimestamp = 0;
  private fireworksCanvas: HTMLCanvasElement | null = null;
  private fireworksCtx: CanvasRenderingContext2D | null = null;
  private fireworksParticles: Array<{ x: number; y: number; vx: number; vy: number; life: number; color: string }> = [];
  private fireworksRaf: number | null = null;
  private lastFireworkBurst = 0;

  private async waitForExistingFunds(address: string, attempts: number, delayMs: number): Promise<boolean> {
    for (let i = 0; i < attempts; i++) {
      await this.loadDevBalance(address);
      if (this.currentBalance > 0n) return true;
      if (i < attempts - 1) {
        await new Promise((r) => setTimeout(r, delayMs));
      }
    }
    return false;
  }

  private getTestDelayBeforeLobbyMs(): number {
    const value = new URLSearchParams(window.location.search).get("testDelayBeforeLobbyMs");
    const parsed = value ? Number(value) : 0;
    return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
  }

  constructor() {
    this.walletManager = getWalletManager();
    this.setupUsernameScreen();
  }

  private setupUsernameScreen(): void {
    const input = document.getElementById("username-input") as HTMLInputElement;
    const confirmBtn = document.getElementById("username-confirm-btn") as HTMLButtonElement;

    input?.addEventListener("input", () => {
      const name = input.value.trim();
      if (confirmBtn) confirmBtn.disabled = name.length === 0;
    });

    const submit = () => {
      const name = input?.value.trim();
      if (!name) return;
      this.username = name;
      this.init();
    };

    confirmBtn?.addEventListener("click", submit);
    input?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") submit();
    });

    input?.focus();
  }

  private async init(): Promise<void> {
    this.showScreen("loading");

    const account = getOrCreateWallet();
    this.currentAccount = account;

    this.setLoadingStatus("Connecting");
    try {
      await getChainClient();
    } catch (e) {
      console.error("Failed to connect to chain:", e);
      this.setLoadingStatus("Failed to connect. Retrying");
      // Retry once after a short delay
      await new Promise((r) => setTimeout(r, 2000));
      await getChainClient();
    }

    const preLobbyDelayMs = this.getTestDelayBeforeLobbyMs();
    if (preLobbyDelayMs > 0) {
      this.setLoadingStatus(`Delaying lobby for test (${preLobbyDelayMs}ms)`);
      await new Promise((r) => setTimeout(r, preLobbyDelayMs));
    }

    const client = await getChainClient();
    const battleshipClient = await BattleshipClient.create(client);
    this.setLoadingStatus("Checking funds");
    const alreadyFunded = await this.waitForExistingFunds(account.address, 3, 1000);

    if (!alreadyFunded) {
      this.setLoadingStatus("Requesting funds");
      try {
        await battleshipClient.requestFunds(account.address);
      } catch (e) {
        console.error("Faucet request failed:", e);
      }

      this.setLoadingStatus("Waiting for funds");
      for (let i = 0; i < 30; i++) {
        await this.loadDevBalance(account.address);
        if (this.currentBalance > 0n) break;
        await new Promise((r) => setTimeout(r, 2000));
      }
    }

    if (this.currentBalance === 0n) {
      this.setLoadingStatus("No funds received. Retrying faucet");
      try {
        await battleshipClient.requestFunds(account.address);
      } catch (e) {
        console.error("Faucet retry failed:", e);
      }
      for (let i = 0; i < 15; i++) {
        await this.loadDevBalance(account.address);
        if (this.currentBalance > 0n) break;
        await new Promise((r) => setTimeout(r, 2000));
      }
    }

    this.onAccountReady();
  }
  private setLoadingStatus(msg: string): void {
    const el = document.getElementById("loading-status");
    if (el) el.textContent = msg;
  }

  private showScreen(screen: Screen): void {
    document.querySelectorAll(".screen").forEach((el) => el.classList.remove("active"));
    document.getElementById("game")?.classList.remove("active");

    if (screen === "game") {
      const gameEl = document.getElementById("game");
      if (gameEl) {
        gameEl.style.display = "block";
        gameEl.classList.add("active");
      }
    } else {
      document.getElementById("game")!.style.display = "none";
      document.getElementById(screen)?.classList.add("active");
    }
  }

  private async loadDevBalance(address: string): Promise<void> {
    try {
      console.log(`[loadDevBalance] Fetching balance for ${address.slice(0, 8)}...`);
      this.currentBalance = await this.walletManager.getBalance(address);
      const formatted = WalletManager.formatBalance(this.currentBalance) + " UNIT";
      console.log(`[loadDevBalance] Balance: ${formatted}`);
      const lobbyEl = document.getElementById("lobby-balance");
      if (lobbyEl) lobbyEl.textContent = formatted;
    } catch (e) {
      console.error("Failed to load dev balance:", e);
    }
  }

  private async onAccountReady(): Promise<void> {
    try {
      const stmtChain = await getStatementChain();
      if (stmtChain) {
        this.statementStore = getStatementStore(stmtChain);
        console.log("[onAccountReady] Statement store initialized");
      } else {
        console.warn("[onAccountReady] Statement store not available (proxy mode?)");
      }
    } catch (e) {
      console.warn("[onAccountReady] Failed to initialize statement store:", e);
    }

    this.showScreen("game-lobby");
    this.setupLobby();
    await this.checkExistingGame();
    await this.refreshGamesList();
    this.startLobbyRefresh();
  }

  private existingGameId: bigint | null = null;
  private existingGamePhase: string | null = null;

  private async checkExistingGame(): Promise<void> {
    if (!this.currentAccount) return;
    try {
      await getChainClient();
      const client = await getChainClient();
      const battleshipClient = await BattleshipClient.create(client);
      const gameId = await battleshipClient.getPlayerGame(this.currentAccount.address);
      this.existingGameId = gameId;
      if (gameId !== null && gameId !== undefined) {
        const { game } = await battleshipClient.getGame(gameId);
        this.existingGamePhase = game?.phase?.type ?? null;
      } else {
        this.existingGamePhase = null;
      }
      this.updateExistingGameUI();
    } catch (e) {
      console.error("Failed to check existing game:", e);
    }
  }

  private updateExistingGameUI(): void {
    let banner = document.getElementById("existing-game-banner");
    if (this.existingGameId !== null && this.existingGameId !== undefined) {
      if (!banner) {
        banner = document.createElement("div");
        banner.id = "existing-game-banner";
        banner.className = "info-box";
        banner.style.cssText = "margin-bottom: 1rem; display: flex; justify-content: space-between; align-items: center;";
        const lobbyActions = document.querySelector(".lobby-actions");
        lobbyActions?.parentNode?.insertBefore(banner, lobbyActions);
      }
      const isWaiting = this.existingGamePhase === "WaitingForOpponent";
      const buttonLabel = isWaiting ? "Cancel" : "Surrender";
      banner.innerHTML = `
        <span>You have an ongoing game (#${this.existingGameId})${isWaiting ? " - waiting for opponent" : ""}</span>
        <button class="btn btn-danger btn-small" id="cancel-existing-btn">${buttonLabel}</button>
      `;
      document.getElementById("cancel-existing-btn")?.addEventListener("click", () => {
        this.cancelOrSurrenderExistingGame();
      });
    } else if (banner) {
      banner.remove();
    }
  }

  private async cancelOrSurrenderExistingGame(): Promise<void> {
    if (!this.currentAccount || this.existingGameId === null) return;
    try {
      await getChainClient();
      const client = await getChainClient();
      const battleshipClient = await BattleshipClient.create(client);
      if (this.existingGamePhase === "WaitingForOpponent") {
        await battleshipClient.cancelGame(this.currentAccount.signer, this.existingGameId);
      } else {
        await battleshipClient.surrender(this.currentAccount.signer, this.existingGameId);
      }
      this.existingGameId = null;
      this.existingGamePhase = null;
      this.updateExistingGameUI();
    } catch (e) {
      console.error("Failed to cancel/surrender:", e);
    }
  }

  private setupLobby(): void {
    if (!this.currentAccount) return;

    const addressEl = document.getElementById("lobby-address");
    const balanceEl = document.getElementById("lobby-balance");

    const lobbyTitle = document.getElementById("lobby-title");
    if (lobbyTitle) lobbyTitle.textContent = `${this.username}'s Lobby`;
    if (addressEl) addressEl.textContent = this.truncateAddress(this.currentAccount.address);
    if (balanceEl) balanceEl.textContent = WalletManager.formatBalance(this.currentBalance) + " UNIT";

    document.getElementById("create-game-btn")?.addEventListener("click", () => {
      this.showFundModal();
    });

    document.getElementById("refresh-lobby-btn")?.addEventListener("click", () => {
      this.refreshGamesList();
    });

    document.getElementById("disconnect-btn")?.addEventListener("click", () => {
      this.disconnect();
    });

    this.setupFundModal();
  }

  private startLobbyRefresh(): void {
    this.stopLobbyRefresh();
    this.lobbyRefreshInterval = window.setInterval(() => {
      this.refreshGamesList();
    }, 5000);
  }

  private stopLobbyRefresh(): void {
    if (this.lobbyRefreshInterval) {
      clearInterval(this.lobbyRefreshInterval);
      this.lobbyRefreshInterval = null;
    }
  }

  private async refreshGamesList(): Promise<void> {
    const gamesList = document.getElementById("games-list");
    const noGamesMsg = document.getElementById("no-games-message");
    if (!gamesList) return;

    try {
      const filteredGames: { creator: string; creatorName?: string; potAmount: bigint; timestamp: number }[] = [];

      if (this.statementStore && this.currentAccount) {
        // Statement-based discovery with ping/pong liveness
        const announcements = this.statementStore.getAnnouncements();
        console.log(`[refreshGamesList] announcements: ${announcements.length}, myAddr: ${this.currentAccount.address.slice(0, 8)}...`);

        for (const ann of announcements) {
          if (ann.creator === this.currentAccount.address) continue;
          if (this.playedGameKeys.has(`${ann.creator}:${ann.timestamp}`)) continue;

          // Send a liveness ping (fire-and-forget)
          this.statementStore.sendLivenessPing(
            ann.creator,
            ann.timestamp,
            this.currentAccount.address,
            this.currentAccount.publicKey!,
            this.currentAccount.rawSign!,
          );

          filteredGames.push({
            creator: ann.creator,
            creatorName: ann.creatorName,
            potAmount: BigInt(ann.potAmount),
            timestamp: ann.timestamp,
          });
        }
      }

      gamesList.querySelectorAll(".game-card:not(.waiting-card)").forEach((el) => el.remove());

      const hasWaitingCard = !!gamesList.querySelector(".waiting-card");

      if (filteredGames.length === 0) {
        if (noGamesMsg) noGamesMsg.style.display = hasWaitingCard ? "none" : "block";
        return;
      }

      if (noGamesMsg) noGamesMsg.style.display = "none";

      filteredGames.forEach((game) => {
        const card = this.createGameCard(game);
        gamesList.appendChild(card);
      });
    } catch (e) {
      console.error("Failed to refresh games list:", e);
    }
  }

  private createGameCard(game: { creator: string; creatorName?: string; potAmount: bigint; timestamp: number }): HTMLElement {
    const card = document.createElement("div");
    card.className = "game-card";

    const stakeFormatted = WalletManager.formatBalance(game.potAmount);
    const prizeFormatted = WalletManager.formatBalance(game.potAmount * 2n);
    const displayName = game.creatorName || this.truncateAddress(game.creator);

    card.innerHTML = `
      <div class="game-id">${displayName}'s game</div>
      <div class="game-stake">Stake: ${stakeFormatted} UNIT</div>
      <div class="game-prize">Winner receives: ${prizeFormatted} UNIT</div>
      <button class="btn btn-success">Join Game</button>
    `;

    const joinBtn = card.querySelector("button");
    joinBtn?.addEventListener("click", () => this.handleJoinGame(game.creator, game.timestamp, game.creatorName));

    return card;
  }

  private setupFundModal(): void {
    const overlay = document.getElementById("fund-modal-overlay");
    const cancelBtn = document.getElementById("cancel-fund-btn");
    const confirmBtn = document.getElementById("confirm-fund-btn");
    const input = document.getElementById("pot-amount-input") as HTMLInputElement;
    const errorEl = document.getElementById("pot-amount-error");

    overlay?.addEventListener("click", () => this.closeFundModal());
    cancelBtn?.addEventListener("click", () => this.closeFundModal());

    input?.addEventListener("input", () => {
      try {
        const amount = WalletManager.parseBalance(input.value);
        const validation = WalletManager.validateStakeAmount(amount, this.currentBalance);

        if (!validation.valid) {
          throw new Error(validation.error);
        }

        this.selectedPotAmount = amount;
        this.updateStakeSummary(amount);

        if (errorEl) {
          errorEl.classList.remove("visible");
          errorEl.textContent = "";
        }
        confirmBtn?.removeAttribute("disabled");
      } catch (e: unknown) {
        const error = e instanceof Error ? e.message : "Invalid input";
        if (errorEl) {
          errorEl.classList.add("visible");
          errorEl.textContent = error;
        }
        confirmBtn?.setAttribute("disabled", "true");
      }
    });

    confirmBtn?.addEventListener("click", () => this.createGameWithStake());
  }

  private showFundModal(): void {
    const modal = document.getElementById("fund-modal");
    const input = document.getElementById("pot-amount-input") as HTMLInputElement;
    const totalBalanceEl = document.getElementById("modal-total-balance");
    const availableBalanceEl = document.getElementById("modal-available-balance");
    const confirmBtn = document.getElementById("confirm-fund-btn");
    const errorEl = document.getElementById("pot-amount-error");

    if (totalBalanceEl) {
      totalBalanceEl.textContent = WalletManager.formatBalance(this.currentBalance) + " UNIT";
    }

    const available = WalletManager.getAvailableBalance(this.currentBalance);
    if (availableBalanceEl) {
      availableBalanceEl.textContent = WalletManager.formatBalance(available) + " UNIT";
    }

    if (input) input.value = "";
    if (errorEl) {
      errorEl.classList.remove("visible");
      errorEl.textContent = "";
    }
    confirmBtn?.setAttribute("disabled", "true");
    this.updateStakeSummary(0n);

    modal?.classList.add("active");
  }

  private closeFundModal(): void {
    const modal = document.getElementById("fund-modal");
    modal?.classList.remove("active");
    this.selectedPotAmount = 0n;
  }

  private updateStakeSummary(amount: bigint): void {
    const formatted = WalletManager.formatBalance(amount);
    const prize = WalletManager.formatBalance(amount * 2n);

    const stakeEl = document.getElementById("stake-display");
    const opponentEl = document.getElementById("opponent-stake-display");
    const prizeEl = document.getElementById("prize-display");

    if (stakeEl) stakeEl.textContent = formatted + " UNIT";
    if (opponentEl) opponentEl.textContent = formatted + " UNIT";
    if (prizeEl) prizeEl.textContent = prize + " UNIT";
  }

  private async createGameWithStake(): Promise<void> {
    console.log("createGameWithStake called, selectedPotAmount:", this.selectedPotAmount);
    if (!this.currentAccount || this.selectedPotAmount <= 0n) return;
    if (!this.statementStore) {
      alert("Statement store not available. Cannot announce game.");
      return;
    }

    const potAmount = this.selectedPotAmount;
    const timestamp = Date.now();
    this.closeFundModal();

    try {
      // Announce game intent via statement store (NO on-chain game yet)
      const announcement: GameAnnouncement = {
        creator: this.currentAccount.address,
        creatorName: this.username,
        potAmount: potAmount.toString(),
        timestamp,
      };
      await this.statementStore.announceGame(announcement, this.currentAccount.publicKey!, this.currentAccount.rawSign!);
      this.activeGameCreator = this.currentAccount.address;
      this.activeGameTimestamp = timestamp;
      console.log("[createGame] Game announced via statement store");

      // Auto-respond to liveness pings while waiting
      this.statementStore.onPing(async (ping) => {
        if (
          ping.creator === this.currentAccount!.address &&
          this.statementStore &&
          this._waitingTimestamp !== null
        ) {
          console.log(`[createGame] Received ping from ${ping.pinger.slice(0, 8)}..., sending pong`);
          await this.statementStore.sendLivenessPong(ping, this.currentAccount!.publicKey!, this.currentAccount!.rawSign!);
        }
      });

      // Listen for join requests — create on-chain when someone wants to join
      this.statementStore.onJoinRequest(async (req) => {
        if (req.creator !== this.currentAccount!.address) return;
        if (req.gameTimestamp !== timestamp) return;
        if (this.playedGameKeys.has(`${req.creator}:${req.gameTimestamp}`)) return;
        if (this._pendingGameId !== null) return;

        console.log(`[createGame] Received join request from ${req.joiner.slice(0, 8)}...`);
        this.opponentName = req.joinerName || this.truncateAddress(req.joiner);
        const statusEl = document.querySelector("#waiting-card .game-status");
        if (statusEl) statusEl.textContent = "Opponent found! Creating game on-chain...";

        // NOW create game on-chain
        const client = await getChainClient();
        const battleshipClient = await BattleshipClient.create(client);
        const { ok, gameId: returnedGameId } = await battleshipClient.createGame(this.currentAccount!.signer, potAmount);

        let gameId = returnedGameId;
        if (ok && gameId === undefined) {
          const queriedId = await battleshipClient.getPlayerGame(this.currentAccount!.address);
          if (queriedId !== null && queriedId !== undefined) gameId = queriedId;
        }

        if (ok && gameId !== undefined) {
          this._pendingGameId = gameId;
          console.log(`[createGame] Game ${gameId} created on-chain`);

          // Notify joiner with the on-chain game ID
          await this.statementStore!.sendGameCreated(
            this.currentAccount!.address,
            timestamp,
            req.joiner,
            gameId.toString(),
            this.currentAccount!.publicKey!,
            this.currentAccount!.rawSign!,
          );
          console.log(`[createGame] Notified ${req.joiner.slice(0, 8)}... of game ${gameId}`);

          // Start polling for opponent to actually join on-chain
          this.waitingCheckInterval = window.setInterval(() => this.checkForOpponentJoined(), 3000);
        } else {
          if (statusEl) statusEl.textContent = "Failed to create game on-chain. Try again.";
        }
      });

      this.showWaitingForOpponent(potAmount, timestamp);
    } catch (e) {
      console.error("Failed to announce game:", e);
      alert("Failed to announce game. Please try again.");
    }
  }

  private _pendingGameId: bigint | null = null;
  private _waitingTimestamp: number | null = null;
  private waitingCheckInterval: number | null = null;

  private showWaitingForOpponent(potAmount: bigint, timestamp: number): void {
    this._waitingTimestamp = timestamp;
    this.stopLobbyRefresh();

    const gamesList = document.getElementById("games-list");
    const noGamesMsg = document.getElementById("no-games-message");
    if (gamesList) {
      gamesList.querySelectorAll(".game-card").forEach((el) => el.remove());
    }
    if (noGamesMsg) noGamesMsg.style.display = "none";

    const waitingCard = document.createElement("div");
    waitingCard.className = "game-card waiting-card";
    waitingCard.id = "waiting-card";
    const stakeFormatted = WalletManager.formatBalance(potAmount);
    waitingCard.innerHTML = `
      <div class="game-id">${this.username}'s game</div>
      <div class="game-stake">Stake: ${stakeFormatted} UNIT</div>
      <div class="game-status">Waiting for opponent to join...</div>
      <button class="btn btn-danger" id="cancel-game-btn">Cancel</button>
    `;
    gamesList?.appendChild(waitingCard);

    document.getElementById("cancel-game-btn")?.addEventListener("click", () => {
      this.cancelWaiting();
    });

    document.getElementById("create-game-btn")?.setAttribute("disabled", "true");
  }

  private async checkForOpponentJoined(): Promise<void> {
    if (this._pendingGameId === null || !this.currentAccount) return;

    try {
      const client = await getChainClient();
      const battleshipClient = await BattleshipClient.create(client);
      const { game } = await battleshipClient.getGame(this._pendingGameId);
      console.log(`[checkForOpponentJoined] game #${this._pendingGameId} phase: ${game?.phase?.type}`);

      if (game && game.phase?.type !== "WaitingForOpponent") {
        console.log("[checkForOpponentJoined] Opponent joined! phase:", game.phase?.type);
        await this.onOpponentJoined(this._pendingGameId);
      }
    } catch (e) {
      console.error("[checkForOpponentJoined] Failed:", e);
    }
  }

  private async onOpponentJoined(gameId: bigint): Promise<void> {
    if (!this.currentAccount) return;

    this.stopWaitingCheck();

    const statusEl = document.querySelector("#waiting-card .game-status");
    if (statusEl) statusEl.textContent = "Opponent found! Starting game...";

    this.game = new OnchainGame("player", this.currentAccount);
    await this.game.initialize();

    this.game.onStateChange(() => this.updateUI());
    this.game.onMessageChange((msg) => this.setStatus(msg));
    this.game.onGameEnd((winner, reason) => this.handleGameEnd(winner, reason));

    const success = await this.game.joinExistingGame(gameId);

    if (success) {
      this._pendingGameId = null;
      this._waitingTimestamp = null;
      this.startGame();
    } else {
      alert("Failed to start game.");
      this.cancelWaiting();
    }
  }

  private stopWaitingCheck(): void {
    if (this.waitingCheckInterval) {
      clearInterval(this.waitingCheckInterval);
      this.waitingCheckInterval = null;
    }
  }

  private async cancelWaiting(): Promise<void> {
    // Cancel on-chain game if one was created
    if (this._pendingGameId !== null && this.currentAccount) {
      try {
        const client = await getChainClient();
        const battleshipClient = await BattleshipClient.create(client);
        await battleshipClient.cancelGame(this.currentAccount.signer, this._pendingGameId);
      } catch (e) {
        console.error("Failed to cancel game on-chain:", e);
      }
    }
    this._pendingGameId = null;
    this._waitingTimestamp = null;
    this.stopWaitingCheck();
    document.getElementById("waiting-card")?.remove();
    document.getElementById("create-game-btn")?.removeAttribute("disabled");
    this.startLobbyRefresh();
    this.refreshGamesList();
  }

  private async handleJoinGame(creator: string, gameTimestamp: number, creatorName?: string): Promise<void> {
    if (!this.currentAccount || !this.statementStore) return;

    this.stopLobbyRefresh();

    try {
      // Send join request via statement store
      console.log(`[handleJoinGame] Sending join request to ${creator.slice(0, 8)}...`);
      this.opponentName = creatorName || this.truncateAddress(creator);
      this.activeGameCreator = creator;
      this.activeGameTimestamp = gameTimestamp;
      await this.statementStore.sendJoinRequest(
        creator,
        gameTimestamp,
        this.currentAccount.address,
        this.currentAccount.publicKey!,
        this.currentAccount.rawSign!,
        this.username,
      );

      // Show waiting UI
      this.setStatus("Waiting for game creator to set up the game on-chain...");

      // Wait for game_created notification (with timeout)
      const gameId = await new Promise<bigint>((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error("Timeout waiting for game creation")), 120_000);
        this.statementStore!.onGameCreated((notification: GameCreatedNotification) => {
          if (
            notification.creator === creator &&
            notification.gameTimestamp === gameTimestamp &&
            notification.joiner === this.currentAccount!.address
          ) {
            clearTimeout(timeout);
            resolve(BigInt(notification.onChainGameId));
          }
        });
      });

      console.log(`[handleJoinGame] Received game ID: ${gameId}, joining on-chain...`);

      // Now join on-chain
      this.game = new OnchainGame("player", this.currentAccount);
      await this.game.initialize();

      this.game.onStateChange(() => this.updateUI());
      this.game.onMessageChange((msg) => this.setStatus(msg));
      this.game.onGameEnd((winner, reason) => this.handleGameEnd(winner, reason));

      const success = await this.game.joinExistingGame(gameId);
      console.log(`[handleJoinGame] joinExistingGame result: ${success}`);

      if (success) {
        this.startGame();
      } else {
        console.log(`[handleJoinGame] join failed, returning to lobby`);
        alert("Failed to join game on-chain.");
        this.game = null;
        this.startLobbyRefresh();
        this.refreshGamesList();
      }
    } catch (e) {
      console.error("[handleJoinGame] Failed to join game:", e);
      alert("Failed to join game. Please try again.");
      this.game = null;
      this.startLobbyRefresh();
      this.refreshGamesList();
    }
  }

  private startGame(): void {
    console.log("[startGame] Showing game screen");
    this.showScreen("game");

    const titleEl = document.getElementById("game-title");
    if (titleEl) {
      titleEl.textContent = `${this.username} vs ${this.opponentName}`;
    }

    this.renderer = new Renderer();
    this.playerCanvas = document.getElementById("player-board") as HTMLCanvasElement;
    this.enemyCanvas = document.getElementById("enemy-board") as HTMLCanvasElement;

    if (this.playerCanvas && this.enemyCanvas) {
      this.playerCtx = this.playerCanvas.getContext("2d");
      this.enemyCtx = this.enemyCanvas.getContext("2d");

      this.playerInput = new InputHandler(this.playerCanvas, this.renderer);
      this.enemyInput = new InputHandler(this.enemyCanvas, this.renderer);

      this.setupInputHandlers();
      this.setupButtons();
    }

    this.gameLoop(0);
  }

  private setupInputHandlers(): void {
    if (!this.playerInput || !this.enemyInput || !this.game) return;

    this.playerInput.onHover((pos) => {
      this.playerHover = pos;
    });

    this.playerInput.onClick((pos) => {
      if (!this.game) return;
      const state = this.game.getState();
      if (state.phase === "setup") {
        this.game.placeShip(pos);
        this.updateButtons();
      }
    });

    this.enemyInput.onHover((pos) => {
      this.enemyHover = pos;
    });

    this.enemyInput.onClick(async (pos) => {
      if (!this.game) return;
      if (this.game.canAttack()) {
        await this.game.attack(pos);
      }
    });

    this.playerInput.onKey((key) => {
      if (!this.game) return;
      const state = this.game.getState();
      if ((key === "r" || key === "R") && state.phase === "setup") {
        this.game.toggleOrientation();
      }
    });
  }

  private setupButtons(): void {
    // Abort previous listeners to prevent duplicate handlers across games
    this.buttonAbortController?.abort();
    this.buttonAbortController = new AbortController();
    const { signal } = this.buttonAbortController;

    const rotateBtn = document.getElementById("rotate-btn");
    const randomBtn = document.getElementById("random-btn");
    const commitBtn = document.getElementById("commit-btn");
    const surrenderBtn = document.getElementById("surrender-btn");

    rotateBtn?.addEventListener("click", () => {
      if (!this.game) return;
      const state = this.game.getState();
      if (state.phase === "setup") {
        this.game.toggleOrientation();
      }
    }, { signal });

    randomBtn?.addEventListener("click", () => {
      if (!this.game) return;
      const state = this.game.getState();
      if (state.phase === "setup") {
        this.game.placeShipsRandomly();
        this.updateButtons();
      }
    }, { signal });

    commitBtn?.addEventListener("click", async () => {
      if (!this.game) return;
      if (this.game.canStartBattle()) {
        await this.game.commitGrid();
        this.updateButtons();
      }
    }, { signal });

    surrenderBtn?.addEventListener("click", async () => {
      if (!this.game) return;
      const state = this.game.getState();
      const canSurrender = state.phase === "setup" || state.phase === "waiting_commit" || state.phase === "battle";
      if (canSurrender) {
        await this.game.surrender();
      }
    }, { signal });
  }

  private updateUI(): void {
    this.updateButtons();
    this.updateInstructions();
  }

  private updateButtons(): void {
    if (!this.game) return;

    const rotateBtn = document.getElementById("rotate-btn") as HTMLButtonElement;
    const randomBtn = document.getElementById("random-btn") as HTMLButtonElement;
    const commitBtn = document.getElementById("commit-btn") as HTMLButtonElement;
    const surrenderBtn = document.getElementById("surrender-btn") as HTMLButtonElement;

    const state = this.game.getState();

    if (rotateBtn) rotateBtn.disabled = state.phase !== "setup";
    if (randomBtn) randomBtn.disabled = state.phase !== "setup";
    if (commitBtn) {
      commitBtn.disabled = state.phase !== "setup" || !this.game.canStartBattle();
    }
    if (surrenderBtn) {
      const canSurrender = state.phase === "setup" || state.phase === "waiting_commit" || state.phase === "battle";
      surrenderBtn.disabled = !canSurrender;
    }
  }

  private updateInstructions(): void {
    if (!this.game) return;

    const instructionsEl = document.getElementById("instructions");
    if (!instructionsEl) return;

    const state = this.game.getState();

    switch (state.phase) {
      case "menu":
        instructionsEl.textContent = "Click 'Join Local Game' to start.";
        break;
      case "creating":
        instructionsEl.textContent = "Creating game...";
        break;
      case "waiting_opponent":
        instructionsEl.textContent = "Waiting for opponent to join...";
        break;
      case "setup":
        instructionsEl.textContent = "Place your ships on your board. Press R to rotate.";
        break;
      case "waiting_commit":
        instructionsEl.textContent = "Waiting for opponent to commit grid...";
        break;
      case "battle":
        instructionsEl.textContent = state.isOurTurn
          ? "Your turn - click enemy waters to attack!"
          : "Opponent's turn...";
        break;
      case "revealing":
        instructionsEl.textContent = "Revealing your grid for verification...";
        break;
      case "finished":
        instructionsEl.textContent =
          state.winner === state.player
            ? "Victory! You won the battle!"
            : "Defeat! Better luck next time.";
        break;
    }
  }

  private setStatus(msg: string): void {
    const statusEl = document.getElementById("status");
    if (statusEl) {
      statusEl.textContent = msg;
    }
  }

  private ensureFireworksCanvas(): void {
    if (this.fireworksCanvas && this.fireworksCtx) return;

    const canvas = document.createElement("canvas");
    canvas.style.position = "fixed";
    canvas.style.inset = "0";
    canvas.style.width = "100vw";
    canvas.style.height = "100vh";
    canvas.style.pointerEvents = "none";
    canvas.style.zIndex = "9999";
    canvas.style.display = "none";
    document.body.appendChild(canvas);

    this.fireworksCanvas = canvas;
    this.fireworksCtx = canvas.getContext("2d");
    this.resizeFireworksCanvas();
    window.addEventListener("resize", () => this.resizeFireworksCanvas());
  }

  private resizeFireworksCanvas(): void {
    if (!this.fireworksCanvas) return;
    this.fireworksCanvas.width = window.innerWidth;
    this.fireworksCanvas.height = window.innerHeight;
  }

  private spawnFireworkBurst(): void {
    if (!this.fireworksCanvas) return;

    const cx = 80 + Math.random() * (this.fireworksCanvas.width - 160);
    const cy = 80 + Math.random() * Math.max(120, this.fireworksCanvas.height * 0.45);
    const colors = ["#ff6b6b", "#ffd93d", "#6bcBff", "#b892ff", "#7dffb3"];

    for (let i = 0; i < 28; i++) {
      const angle = (Math.PI * 2 * i) / 28 + Math.random() * 0.2;
      const speed = 1.5 + Math.random() * 4;
      this.fireworksParticles.push({
        x: cx,
        y: cy,
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed - 1.5,
        life: 40 + Math.random() * 20,
        color: colors[Math.floor(Math.random() * colors.length)],
      });
    }
  }

  private startVictoryFireworks(): void {
    this.ensureFireworksCanvas();
    if (!this.fireworksCanvas || !this.fireworksCtx) return;

    this.fireworksCanvas.style.display = "block";
    this.fireworksParticles = [];
    this.lastFireworkBurst = 0;

    if (this.fireworksRaf !== null) {
      cancelAnimationFrame(this.fireworksRaf);
    }

    const tick = (time: number) => {
      if (!this.fireworksCanvas || !this.fireworksCtx) return;

      if (time - this.lastFireworkBurst > 220) {
        this.spawnFireworkBurst();
        this.lastFireworkBurst = time;
      }

      const ctx = this.fireworksCtx;
      ctx.clearRect(0, 0, this.fireworksCanvas.width, this.fireworksCanvas.height);

      this.fireworksParticles = this.fireworksParticles.filter((particle) => particle.life > 0);
      for (const particle of this.fireworksParticles) {
        particle.x += particle.vx;
        particle.y += particle.vy;
        particle.vy += 0.04;
        particle.vx *= 0.99;
        particle.life -= 1;

        ctx.globalAlpha = Math.max(0, particle.life / 60);
        ctx.fillStyle = particle.color;
        ctx.beginPath();
        ctx.arc(particle.x, particle.y, 2.2, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;

      this.fireworksRaf = requestAnimationFrame(tick);
    };

    this.fireworksRaf = requestAnimationFrame(tick);

    window.setTimeout(() => this.stopVictoryFireworks(), 2600);
  }

  private stopVictoryFireworks(): void {
    if (this.fireworksRaf !== null) {
      cancelAnimationFrame(this.fireworksRaf);
      this.fireworksRaf = null;
    }
    this.fireworksParticles = [];
    if (this.fireworksCanvas && this.fireworksCtx) {
      this.fireworksCtx.clearRect(0, 0, this.fireworksCanvas.width, this.fireworksCanvas.height);
      this.fireworksCanvas.style.display = "none";
    }
  }

  private handleGameEnd(winner: Player | null, reason: string): void {
    console.log(`Game ended: winner=${winner}, reason=${reason}`);
    if (this.activeGameCreator && this.activeGameTimestamp) {
      const key = `${this.activeGameCreator}:${this.activeGameTimestamp}`;
      this.playedGameKeys.add(key);
      this.statementStore?.removeAnnouncement(this.activeGameCreator, this.activeGameTimestamp);
      this.activeGameCreator = "";
      this.activeGameTimestamp = 0;
    }
    if (winner === "player") {
      this.startVictoryFireworks();
    }
    setTimeout(() => {
      this.returnToLobby();
    }, 3000);
  }

  private returnToLobby(): void {
    console.log('[returnToLobby] Cleaning up and returning to lobby');
    this.stopVictoryFireworks();
    if (this.game) {
      this.game.reset();
      this.game = null;
    }

    // Abort game button listeners to prevent duplicate handlers in next game
    this.buttonAbortController?.abort();
    this.buttonAbortController = null;

    // Reset nonce cache to avoid stale nonce issues in next game
    if (this.currentAccount) {
      resetLocalNonce(this.currentAccount.address);
    }

    this.playerHover = null;
    this.enemyHover = null;
    this.opponentName = "";

    this.showScreen("game-lobby");
    document.getElementById("create-game-btn")?.removeAttribute("disabled");
    document.getElementById("waiting-card")?.remove();
    document.getElementById("join-waiting-card")?.remove();
    this.checkExistingGame();
    this.refreshGamesList();
    this.startLobbyRefresh();
    this.refreshBalance();
  }

  private async refreshBalance(): Promise<void> {
    if (!this.currentAccount) return;

    try {
      this.currentBalance = await this.walletManager.getBalance(this.currentAccount.address);
      const balanceEl = document.getElementById("lobby-balance");
      if (balanceEl) {
        balanceEl.textContent = WalletManager.formatBalance(this.currentBalance) + " UNIT";
      }
    } catch (e) {
      console.error("Failed to refresh balance:", e);
    }
  }

  private disconnect(): void {
    if (this.statementStore) {
      this.statementStore.destroy();
      this.statementStore = null;
    }
    this.stopLobbyRefresh();
    this.walletManager.disconnect();
    this.currentAccount = null;
    this.currentBalance = 0n;
    window.location.reload();
  }

  private gameLoop(time: number): void {
    const deltaTime = time - this.lastTime;
    this.lastTime = time;

    if (this.renderer) {
      this.renderer.update(deltaTime);
    }
    this.render();

    requestAnimationFrame((t) => this.gameLoop(t));
  }

  private render(): void {
    if (!this.game || !this.renderer || !this.playerCtx || !this.enemyCtx) return;

    const state = this.game.getState();

    let placementPreview = null;
    if (state.phase === "setup" && this.playerHover) {
      const currentShip = this.game.getCurrentShip();
      if (currentShip) {
        placementPreview = {
          definition: currentShip,
          position: this.playerHover,
          orientation: this.game.getPlacementOrientation(),
          valid: this.game.canPlaceCurrentShip(this.playerHover),
        };
      }
    }

    this.renderer.renderPlayerBoard(
      this.playerCtx,
      this.game.getOurBoard(),
      state.phase === "setup" ? this.playerHover : null,
      placementPreview
    );

    const canAttack = this.game.canAttack();
    const enemyCanvas = document.getElementById("enemy-board");
    if (enemyCanvas) enemyCanvas.dataset.canAttack = String(canAttack);
    this.renderer.renderEnemyBoard(
      this.enemyCtx,
      this.game.getOpponentBoard(),
      this.enemyHover,
      canAttack
    );
  }

  private truncateAddress(address: string): string {
    return `${address.slice(0, 6)}...${address.slice(-4)}`;
  }
}

new BattleshipApp();
