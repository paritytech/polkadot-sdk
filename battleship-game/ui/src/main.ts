import { OnchainGame } from "./game/OnchainGame.ts";
import { Renderer } from "./render/Renderer.ts";
import { InputHandler } from "./input/InputHandler.ts";
import { getPlayerFromUrl } from "./chain/accounts.ts";
import type { Position } from "./types/index.ts";

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

  constructor() {
    this.setupMenu();
  }

  private setupMenu(): void {
    const player = getPlayerFromUrl();
    const playerNameEl = document.getElementById("player-name");
    if (playerNameEl) {
      playerNameEl.textContent = player.toUpperCase();
    }

    const localGameBtn = document.getElementById("local-game-btn");
    localGameBtn?.addEventListener("click", () => this.startLocalGame());

    const multiplayerBtn = document.getElementById("multiplayer-btn");
    if (multiplayerBtn) {
      multiplayerBtn.setAttribute("disabled", "true");
    }
  }

  private async startLocalGame(): Promise<void> {
    const player = getPlayerFromUrl();

    // Initialize game
    this.game = new OnchainGame(player);
    await this.game.initialize();

    // Setup callbacks
    this.game.onStateChange(() => this.updateUI());
    this.game.onMessageChange((msg) => this.setStatus(msg));
    this.game.onGameEnd((winner, reason) => this.handleGameEnd(winner, reason));

    // Hide menu, show game
    const menuEl = document.getElementById("menu");
    const gameEl = document.getElementById("game");
    if (menuEl) menuEl.style.display = "none";
    if (gameEl) gameEl.style.display = "block";

    // Initialize rendering
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

    // Create or join game based on player
    if (player === "alice") {
      await this.game.createGame();
    } else {
      // Bob tries to join
      const joined = await this.game.joinGame();
      if (!joined) {
        // No game available, wait and retry
        this.setStatus("Waiting for Alice to create a game...");
        this.waitForGame();
      }
    }

    // Start render loop
    this.gameLoop(0);
  }

  private async waitForGame(): Promise<void> {
    // Poll for available games
    const checkInterval = setInterval(async () => {
      if (this.game) {
        const joined = await this.game.joinGame();
        if (joined) {
          clearInterval(checkInterval);
        }
      }
    }, 2000);
  }

  private setupInputHandlers(): void {
    if (!this.playerInput || !this.enemyInput || !this.game) return;

    // Player board interactions (placement)
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

    // Enemy board interactions (attacks)
    this.enemyInput.onHover((pos) => {
      this.enemyHover = pos;
    });

    this.enemyInput.onClick(async (pos) => {
      if (!this.game) return;
      if (this.game.canAttack()) {
        await this.game.attack(pos);
      }
    });

    // Keyboard controls
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
        console.log(`[${state.player}] Surrendering game #${state.gameId}...`);
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
      // Enable surrender for setup, waiting_commit, and battle phases
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
        instructionsEl.textContent =
          "Place your ships on your board. Press R to rotate.";
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
    const player = getPlayerFromUrl();
    console.log(`Game ended: winner=${winner}, reason=${reason}, player=${player}`);

    // Show result for 3 seconds, then return to menu
    setTimeout(() => {
      this.returnToMenu();
    }, 3000);
  }

  private returnToMenu(): void {
    // Reset game state
    if (this.game) {
      this.game.reset();
    }

    // Show menu, hide game
    const menuEl = document.getElementById("menu");
    const gameEl = document.getElementById("game");
    if (menuEl) menuEl.style.display = "block";
    if (gameEl) gameEl.style.display = "none";

    this.setStatus("");
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

    // Determine placement preview
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

    // Render player board
    this.renderer.renderPlayerBoard(
      this.playerCtx,
      this.game.getOurBoard(),
      state.phase === "setup" ? this.playerHover : null,
      placementPreview
    );

    // Render enemy board
    const canAttack = this.game.canAttack();
    this.renderer.renderEnemyBoard(
      this.enemyCtx,
      this.game.getOpponentBoard(),
      this.enemyHover,
      canAttack
    );
  }
}

// Start the app
new BattleshipApp();
