import { OnchainGame } from "./game/OnchainGame.ts";
import { Renderer } from "./render/Renderer.ts";
import { InputHandler } from "./input/InputHandler.ts";
import { isDevMode, getPlayerFromUrl, getDevPlayerAccount, type PlayerAccount } from "./chain/accounts.ts";
import { getWalletManager, WalletManager, type WalletInfo, type WalletAccount } from "./chain/wallet.ts";
import { getChainClient } from "./chain/client.ts";
import { getStatementStore, type GameAnnouncement } from "./chain/statementStore.ts";
import type { Position, Player } from "./types/index.ts";

type Screen = "wallet-connect" | "game-lobby" | "game";

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
  private devModePlayer: Player = "alice";

  constructor() {
    this.walletManager = getWalletManager();
    this.init();
  }

  private init(): void {
    if (isDevMode()) {
      this.setupDevMode();
    } else {
      this.setupWalletConnect();
    }
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

  private setupDevMode(): void {
    this.devModePlayer = getPlayerFromUrl();
    const devToggle = document.getElementById("dev-mode-toggle") as HTMLInputElement;
    if (devToggle) devToggle.checked = true;

    this.toggleDevModeUI(true);
    this.selectDevPlayer(this.devModePlayer);

    devToggle?.addEventListener("change", () => {
      if (!devToggle.checked) {
        window.location.search = "";
      }
    });

    document.querySelectorAll("#dev-mode-section .card").forEach((card) => {
      card.addEventListener("click", () => {
        const player = (card as HTMLElement).dataset.player as Player;
        this.selectDevPlayer(player);
      });
    });

    document.getElementById("wallet-continue-btn")?.addEventListener("click", () => {
      this.onAccountReady();
    });
  }

  private toggleDevModeUI(devMode: boolean): void {
    const devSection = document.getElementById("dev-mode-section");
    const walletSection = document.getElementById("wallet-section");
    const accountSection = document.getElementById("account-section");

    if (devSection) devSection.style.display = devMode ? "block" : "none";
    if (walletSection) walletSection.style.display = devMode ? "none" : "block";
    if (accountSection) accountSection.style.display = "none";
  }

  private selectDevPlayer(player: Player): void {
    this.devModePlayer = player;
    const account = getDevPlayerAccount(player);
    this.currentAccount = account;

    document.querySelectorAll("#dev-mode-section .card").forEach((card) => {
      card.classList.toggle("selected", (card as HTMLElement).dataset.player === player);
    });

    this.updateSelectedAccountDisplay(account.address, "Dev Account");
    document.getElementById("wallet-continue-btn")?.removeAttribute("disabled");

    this.loadDevBalance(account.address);
  }

  private async loadDevBalance(address: string): Promise<void> {
    try {
      this.currentBalance = await this.walletManager.getBalance(address);
      const balanceEl = document.getElementById("selected-balance");
      if (balanceEl) {
        balanceEl.textContent = WalletManager.formatBalance(this.currentBalance) + " UNIT";
      }
    } catch (e) {
      console.error("Failed to load dev balance:", e);
    }
  }

  private setupWalletConnect(): void {
    const devToggle = document.getElementById("dev-mode-toggle") as HTMLInputElement;

    devToggle?.addEventListener("change", () => {
      if (devToggle.checked) {
        window.location.search = "?devMode=true";
      } else {
        this.toggleDevModeUI(false);
      }
    });

    this.loadWallets();

    document.getElementById("wallet-continue-btn")?.addEventListener("click", () => {
      this.onAccountReady();
    });
  }

  private loadWallets(): void {
    const walletList = document.getElementById("wallet-list");
    const noWalletsMsg = document.getElementById("no-wallets-message");
    if (!walletList) return;

    const wallets = this.walletManager.detectWallets();

    if (wallets.length === 0) {
      if (noWalletsMsg) noWalletsMsg.style.display = "block";
      return;
    }

    if (noWalletsMsg) noWalletsMsg.style.display = "none";

    wallets.forEach((wallet) => {
      const card = document.createElement("div");
      card.className = "card";
      card.innerHTML = `
        <div class="card-icon">${this.getWalletIcon(wallet.name)}</div>
        <div class="card-title">${wallet.displayName}</div>
      `;
      card.addEventListener("click", () => this.connectWallet(wallet));
      walletList.appendChild(card);
    });
  }

  private getWalletIcon(walletName: string): string {
    const icons: Record<string, string> = {
      "polkadot-js": "🔴",
      "talisman": "🌙",
      "subwallet-js": "📱",
      "enkrypt": "🔐",
    };
    return icons[walletName] || "💳";
  }

  private async connectWallet(wallet: WalletInfo): Promise<void> {
    const success = await this.walletManager.connect(wallet.name);
    if (!success) {
      alert("Failed to connect to wallet. Please try again.");
      return;
    }

    document.querySelectorAll("#wallet-list .card").forEach((card) => {
      card.classList.toggle("selected", card.querySelector(".card-title")?.textContent === wallet.displayName);
    });

    this.showAccountSelection();
  }

  private async showAccountSelection(): Promise<void> {
    const accountSection = document.getElementById("account-section");
    const accountList = document.getElementById("account-list");
    if (!accountSection || !accountList) return;

    accountSection.style.display = "block";
    accountList.innerHTML = "";

    const accounts = this.walletManager.getAccounts();

    for (const account of accounts) {
      const balance = await this.walletManager.getBalance(account.address);
      const formatted = WalletManager.formatBalance(balance);

      const card = document.createElement("div");
      card.className = "card";
      card.innerHTML = `
        <div class="card-title">${account.name}</div>
        <div class="card-subtitle">${this.truncateAddress(account.address)}</div>
        <div class="card-balance">${formatted} UNIT</div>
      `;
      card.addEventListener("click", () => this.selectWalletAccount(account, balance));
      accountList.appendChild(card);
    }
  }

  private selectWalletAccount(account: WalletAccount, balance: bigint): void {
    this.walletManager.selectAccount(account.address);
    this.currentAccount = {
      address: account.address,
      signer: account.signer,
    };
    this.currentBalance = balance;

    document.querySelectorAll("#account-list .card").forEach((card) => {
      const subtitle = card.querySelector(".card-subtitle")?.textContent;
      card.classList.toggle("selected", subtitle === this.truncateAddress(account.address));
    });

    this.updateSelectedAccountDisplay(account.address, WalletManager.formatBalance(balance) + " UNIT");
    document.getElementById("wallet-continue-btn")?.removeAttribute("disabled");
  }

  private updateSelectedAccountDisplay(address: string, balance: string): void {
    const info = document.getElementById("selected-account-info");
    const addressEl = document.getElementById("selected-address");
    const balanceEl = document.getElementById("selected-balance");

    if (info) info.style.display = "flex";
    if (addressEl) addressEl.textContent = this.truncateAddress(address);
    if (balanceEl) balanceEl.textContent = balance;
  }

  private async onAccountReady(): Promise<void> {
    this.showScreen("game-lobby");
    this.setupLobby();
    await this.refreshGamesList();
    this.startLobbyRefresh();
  }

  private setupLobby(): void {
    if (!this.currentAccount) return;

    const addressEl = document.getElementById("lobby-address");
    const balanceEl = document.getElementById("lobby-balance");

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
      const client = await getChainClient();
      const statementStore = getStatementStore(client);
      const games = await statementStore.getAvailableGames();
      const filteredGames = games.filter((g) => g.creator !== this.currentAccount?.address);

      gamesList.querySelectorAll(".game-card").forEach((el) => el.remove());

      if (filteredGames.length === 0) {
        if (noGamesMsg) noGamesMsg.style.display = "block";
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

  private createGameCard(game: GameAnnouncement): HTMLElement {
    const card = document.createElement("div");
    card.className = "game-card";

    const potAmount = BigInt(game.potAmount);
    const stakeFormatted = WalletManager.formatBalance(potAmount);
    const prizeFormatted = WalletManager.formatBalance(potAmount * 2n);

    card.innerHTML = `
      <div class="game-id">Game by ${this.truncateAddress(game.creator)}</div>
      <div class="game-stake">Stake: ${stakeFormatted} UNIT</div>
      <div class="game-prize">Winner receives: ${prizeFormatted} UNIT</div>
      <button class="btn btn-success">Join Game</button>
    `;

    const joinBtn = card.querySelector("button");
    joinBtn?.addEventListener("click", () => this.joinGame(game));

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

    const potAmount = this.selectedPotAmount;
    this.closeFundModal();

    try {
      const client = await getChainClient();
      const statementStore = getStatementStore(client);

      const announcement: GameAnnouncement = {
        creator: this.currentAccount.address,
        potAmount: potAmount.toString(),
        timestamp: Date.now(),
      };
      console.log("Announcing game:", announcement);

      const success = await statementStore.announceGame(announcement, this.currentAccount.signer);

      if (success) {
        this.showWaitingForOpponent(announcement);
      } else {
        alert("Failed to announce game. Please try again.");
      }
    } catch (e) {
      console.error("Failed to create game:", e);
      alert("Failed to announce game. Please try again.");
    }
  }

  private _pendingAnnouncement: GameAnnouncement | null = null;
  private waitingCheckInterval: number | null = null;

  private showWaitingForOpponent(announcement: GameAnnouncement): void {
    this._pendingAnnouncement = announcement;
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
    const stakeFormatted = WalletManager.formatBalance(BigInt(announcement.potAmount));
    waitingCard.innerHTML = `
      <div class="game-id">Your Game</div>
      <div class="game-stake">Stake: ${stakeFormatted} UNIT</div>
      <div class="game-status">Waiting for opponent to join...</div>
      <button class="btn btn-danger" id="cancel-game-btn">Cancel</button>
    `;
    gamesList?.appendChild(waitingCard);

    document.getElementById("cancel-game-btn")?.addEventListener("click", () => {
      this.cancelWaiting();
    });

    document.getElementById("create-game-btn")?.setAttribute("disabled", "true");

    this.waitingCheckInterval = window.setInterval(() => this.checkForJoinResponses(), 3000);
  }

  private async checkForJoinResponses(): Promise<void> {
    if (!this._pendingAnnouncement || !this.currentAccount) return;

    try {
      const client = await getChainClient();
      const statementStore = getStatementStore(client);
      const responses = await statementStore.getJoinResponses(
        this._pendingAnnouncement.creator,
        this._pendingAnnouncement.timestamp
      );

      if (responses.length > 0) {
        const joiner = responses[0];
        console.log("Join response received from:", joiner.joiner);
        await this.acceptJoinResponse(joiner);
      }
    } catch (e) {
      console.error("Failed to check join responses:", e);
    }
  }

  private async acceptJoinResponse(_response: { joiner: string; timestamp: number }): Promise<void> {
    if (!this._pendingAnnouncement || !this.currentAccount) return;

    this.stopWaitingCheck();

    const statusEl = document.querySelector("#waiting-card .game-status");
    if (statusEl) statusEl.textContent = "Opponent found! Creating game...";

    const player = isDevMode() ? this.devModePlayer : "player";
    this.game = new OnchainGame(player, this.currentAccount);
    await this.game.initialize();

    this.game.onStateChange(() => this.updateUI());
    this.game.onMessageChange((msg) => this.setStatus(msg));
    this.game.onGameEnd((winner, reason) => this.handleGameEnd(winner, reason));

    const potAmount = BigInt(this._pendingAnnouncement.potAmount);
    const success = await this.game.createGame(potAmount);

    if (success) {
      const gameState = this.game.getState();
      if (gameState.gameId) {
        const client = await getChainClient();
        const statementStore = getStatementStore(client);
        const updatedAnnouncement: GameAnnouncement = {
          ...this._pendingAnnouncement,
          onChainGameId: gameState.gameId.toString(),
        };
        await statementStore.announceGame(updatedAnnouncement, this.currentAccount.signer, 101);
      }
      this.startGame();
    } else {
      alert("Failed to create on-chain game.");
      this.cancelWaiting();
    }
  }

  private stopWaitingCheck(): void {
    if (this.waitingCheckInterval) {
      clearInterval(this.waitingCheckInterval);
      this.waitingCheckInterval = null;
    }
  }

  private cancelWaiting(): void {
    console.log("Cancelling wait for:", this._pendingAnnouncement?.creator);
    this._pendingAnnouncement = null;
    this.stopWaitingCheck();
    document.getElementById("waiting-card")?.remove();
    document.getElementById("create-game-btn")?.removeAttribute("disabled");
    this.startLobbyRefresh();
    this.refreshGamesList();
  }

  private joiningGame: GameAnnouncement | null = null;
  private joinCheckInterval: number | null = null;

  private async joinGame(game: GameAnnouncement): Promise<void> {
    if (!this.currentAccount) return;

    const potAmount = BigInt(game.potAmount);
    if (this.currentBalance < potAmount) {
      alert("Insufficient balance to join this game");
      return;
    }

    this.stopLobbyRefresh();

    const client = await getChainClient();
    const statementStore = getStatementStore(client);

    const success = await statementStore.sendJoinResponse(
      game.creator,
      game.timestamp,
      this.currentAccount.address,
      this.currentAccount.signer
    );

    if (success) {
      this.joiningGame = game;
      this.showWaitingForGameCreation(game);
      this.joinCheckInterval = window.setInterval(() => this.checkForOnChainGame(), 3000);
    } else {
      alert("Failed to send join request.");
      this.startLobbyRefresh();
    }
  }

  private showWaitingForGameCreation(game: GameAnnouncement): void {
    const gamesList = document.getElementById("games-list");
    const noGamesMsg = document.getElementById("no-games-message");
    if (gamesList) {
      gamesList.querySelectorAll(".game-card").forEach((el) => el.remove());
    }
    if (noGamesMsg) noGamesMsg.style.display = "none";

    const waitingCard = document.createElement("div");
    waitingCard.className = "game-card waiting-card";
    waitingCard.id = "join-waiting-card";
    const stakeFormatted = WalletManager.formatBalance(BigInt(game.potAmount));
    waitingCard.innerHTML = `
      <div class="game-id">Joining ${this.truncateAddress(game.creator)}'s game</div>
      <div class="game-stake">Stake: ${stakeFormatted} UNIT</div>
      <div class="game-status">Waiting for host to create game...</div>
      <button class="btn btn-danger" id="cancel-join-btn">Cancel</button>
    `;
    gamesList?.appendChild(waitingCard);

    document.getElementById("cancel-join-btn")?.addEventListener("click", () => {
      this.cancelJoin();
    });

    document.getElementById("create-game-btn")?.setAttribute("disabled", "true");
  }

  private async checkForOnChainGame(): Promise<void> {
    if (!this.joiningGame || !this.currentAccount) return;

    try {
      const client = await getChainClient();
      const statementStore = getStatementStore(client);
      const games = await statementStore.getAvailableGames();

      const updatedGame = games.find(
        (g) =>
          g.creator === this.joiningGame?.creator &&
          g.timestamp === this.joiningGame?.timestamp &&
          g.onChainGameId
      );

      if (updatedGame?.onChainGameId) {
        console.log("On-chain game found:", updatedGame.onChainGameId);
        await this.joinOnChainGame(BigInt(updatedGame.onChainGameId));
      }
    } catch (e) {
      console.error("Failed to check for on-chain game:", e);
    }
  }

  private async joinOnChainGame(onChainGameId: bigint): Promise<void> {
    if (!this.currentAccount || !this.joiningGame) return;

    this.stopJoinCheck();

    const statusEl = document.querySelector("#join-waiting-card .game-status");
    if (statusEl) statusEl.textContent = "Game found! Joining...";

    const player = isDevMode() ? this.devModePlayer : "player";
    this.game = new OnchainGame(player, this.currentAccount);
    await this.game.initialize();

    this.game.onStateChange(() => this.updateUI());
    this.game.onMessageChange((msg) => this.setStatus(msg));
    this.game.onGameEnd((winner, reason) => this.handleGameEnd(winner, reason));

    const success = await this.game.joinExistingGame(onChainGameId);

    if (success) {
      this.joiningGame = null;
      this.startGame();
    } else {
      alert("Failed to join on-chain game.");
      this.cancelJoin();
    }
  }

  private stopJoinCheck(): void {
    if (this.joinCheckInterval) {
      clearInterval(this.joinCheckInterval);
      this.joinCheckInterval = null;
    }
  }

  private cancelJoin(): void {
    this.joiningGame = null;
    this.stopJoinCheck();
    document.getElementById("join-waiting-card")?.remove();
    document.getElementById("create-game-btn")?.removeAttribute("disabled");
    this.startLobbyRefresh();
    this.refreshGamesList();
  }

  private startGame(): void {
    this.showScreen("game");

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
    });

    randomBtn?.addEventListener("click", () => {
      if (!this.game) return;
      const state = this.game.getState();
      if (state.phase === "setup") {
        this.game.placeShipsRandomly();
        this.updateButtons();
      }
    });

    commitBtn?.addEventListener("click", async () => {
      if (!this.game) return;
      if (this.game.canStartBattle()) {
        await this.game.commitGrid();
        this.updateButtons();
      }
    });

    surrenderBtn?.addEventListener("click", async () => {
      if (!this.game) return;
      const state = this.game.getState();
      const canSurrender = state.phase === "setup" || state.phase === "waiting_commit" || state.phase === "battle";
      if (canSurrender) {
        await this.game.surrender();
      }
    });
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

  private handleGameEnd(winner: string, reason: string): void {
    console.log(`Game ended: winner=${winner}, reason=${reason}`);
    setTimeout(() => {
      this.returnToLobby();
    }, 3000);
  }

  private returnToLobby(): void {
    if (this.game) {
      this.game.reset();
      this.game = null;
    }

    this.playerHover = null;
    this.enemyHover = null;

    this.showScreen("game-lobby");
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
    this.stopLobbyRefresh();
    this.walletManager.disconnect();
    this.currentAccount = null;
    this.currentBalance = 0n;

    if (isDevMode()) {
      window.location.search = "";
    } else {
      window.location.reload();
    }
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
