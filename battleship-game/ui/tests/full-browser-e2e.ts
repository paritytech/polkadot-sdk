import { chromium, Page } from "playwright";
import { startProxy } from "../src/chain/smoldot-proxy.ts";

const BASE_URL = "http://localhost:3000";
const RPC_URL = process.env.WS_URL || "ws://localhost:9944";
const CHROMIUM_PATH = "/nix/store/g245pzpbacazlrca1fb7crb9883rhhs3-chromium-144.0.7559.59/bin/chromium";
const PROXY_PORT = 9999;

async function waitForText(page: Page, text: string, timeout = 30000) {
  await page.waitForFunction(
    (t) => document.body.textContent?.includes(t),
    text,
    { timeout }
  );
}

async function waitForSelector(page: Page, selector: string, timeout = 30000) {
  await page.waitForSelector(selector, { state: "visible", timeout });
}

async function selectDevPlayer(page: Page, player: "alice" | "bob") {
  await page.click(`[data-player="${player}"]`);
  await page.click("#wallet-continue-btn");

  // Wait for either lobby or game screen
  for (let i = 0; i < 60; i++) {
    const lobby = await page.locator("#game-lobby.active").isVisible().catch(() => false);
    const game = await page.locator("#game.active").isVisible().catch(() => false);
    if (lobby || game) break;
    if (i === 59) {
      const activeScreens = await page.evaluate(() =>
        Array.from(document.querySelectorAll(".screen.active")).map(s => s.id)
      ).catch(() => []);
      throw new Error(`${player} failed to reach lobby/game - screens: ${JSON.stringify(activeScreens)}`);
    }
    await page.waitForTimeout(1000);
  }
  await page.waitForTimeout(500);

  // Handle existing game: click cancel/surrender and wait for lobby to clear
  const existingGameBanner = await page.locator("#existing-game-banner").isVisible().catch(() => false);
  if (existingGameBanner) {
    console.log(`  ${player} has existing game, clicking cancel/surrender...`);
    const abandonBtn = page.locator("#existing-game-banner button");
    if (await abandonBtn.isVisible().catch(() => false)) {
      await abandonBtn.click();
      // Wait for the tx to take effect and banner to disappear
      for (let i = 0; i < 30; i++) {
        await page.waitForTimeout(2000);
        const stillThere = await page.locator("#existing-game-banner").isVisible().catch(() => false);
        if (!stillThere) {
          console.log(`  ${player} existing game cleared`);
          break;
        }
        if (i === 29) console.log(`  ${player} existing game banner persists, continuing anyway`);
      }
    }
  }

  const inGameScreen = await page.locator("#game.active").isVisible().catch(() => false);
  if (inGameScreen) {
    console.log(`  ${player} in game screen, surrendering...`);
    const surrenderBtn = page.locator("#surrender-btn");
    for (let i = 0; i < 20; i++) {
      if (await surrenderBtn.isEnabled().catch(() => false)) {
        await surrenderBtn.click();
        console.log(`  ${player} clicked surrender`);
        // Wait for game to end and return to lobby
        await page.waitForTimeout(10000);
        break;
      }
      await page.waitForTimeout(500);
    }
  }

  await page.waitForSelector("#game-lobby.active", { state: "visible", timeout: 120000 });
}

async function createGame(page: Page, stake: string = "1") {
  await page.waitForFunction(
    () => {
      const el = document.getElementById("lobby-balance");
      return el && el.textContent && !el.textContent.includes("0.000 UNIT");
    },
    null,
    { timeout: 120000 }
  );
  await page.click("#create-game-btn");
  await waitForSelector(page, "#fund-modal.active");
  await page.fill("#pot-amount-input", stake);
  await page.waitForTimeout(300);
  await page.click("#confirm-fund-btn");
  await waitForText(page, "Waiting for opponent", 120000);
}

async function joinGame(page: Page) {
  await page.waitForFunction(
    () => {
      const el = document.getElementById("lobby-balance");
      return el && el.textContent && !el.textContent.includes("0.000 UNIT");
    },
    null,
    { timeout: 120000 }
  );
  for (let i = 0; i < 60; i++) {
    await page.waitForTimeout(2000);
    await page.click("#refresh-lobby-btn").catch(() => {});
    await page.waitForTimeout(1000);
    const gameCard = page.locator(".game-card").first();
    if (await gameCard.isVisible().catch(() => false)) {
      const joinBtn = gameCard.locator("button", { hasText: "Join Game" });
      if (await joinBtn.isVisible().catch(() => false)) {
        await joinBtn.click();
        await page.waitForTimeout(5000);
        return true;
      }
    }
  }
  throw new Error("Could not find game to join");
}

async function waitForGameScreen(page: Page) {
  for (let i = 0; i < 60; i++) {
    const isActive = await page.locator("#game.active").isVisible().catch(() => false);
    if (isActive) return;
    
    const activeScreens = await page.evaluate(() => 
      Array.from(document.querySelectorAll(".screen.active, #game.active")).map(s => s.id)
    ).catch(() => []);
    
    if (i % 5 === 0) {
      console.log(`  Waiting for game screen... active: ${JSON.stringify(activeScreens)}`);
    }
    
    await page.waitForTimeout(2000);
  }
  throw new Error("Timeout waiting for game screen");
}

async function waitForSetupPhase(page: Page) {
  await page.waitForFunction(
    () => {
      const instructions = document.getElementById("instructions");
      return instructions?.textContent?.includes("Place your ships");
    },
    null,
    { timeout: 120000 }
  );
}

async function placeShipsRandomly(page: Page) {
  const randomBtn = page.locator("#random-btn");
  await randomBtn.waitFor({ state: "visible" });
  
  for (let attempt = 0; attempt < 3; attempt++) {
    if (await randomBtn.isDisabled()) {
      break;
    }
    await randomBtn.click();
    await page.waitForTimeout(500);
  }
}

async function commitGrid(page: Page) {
  const commitBtn = page.locator("#commit-btn");
  await commitBtn.waitFor({ state: "visible" });
  
  for (let i = 0; i < 30; i++) {
    if (!(await commitBtn.isDisabled())) {
      await commitBtn.click();
      
      await page.waitForFunction(
        () => {
          const status = document.getElementById("status");
          const text = status?.textContent || "";
          return text.includes("Waiting for opponent to commit") || 
                 text.includes("Your turn") || 
                 text.includes("Opponent's turn");
        },
        null,
        { timeout: 60000 }
      );
      return true;
    }
    await page.waitForTimeout(500);
  }
  throw new Error("Commit button never became enabled");
}

async function waitForBattlePhase(page: Page) {
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      const text = status?.textContent || "";
      return text.includes("Your turn") || 
             text.includes("Opponent's turn") || 
             text.includes("Waiting for opponent to reveal") ||
             text.includes("click enemy waters");
    },
    null,
    { timeout: 120000 }
  );
}

async function isOurTurn(page: Page): Promise<boolean> {
  const status = await page.locator("#status").textContent();
  return status?.includes("Your turn") || false;
}

async function isGameOver(page: Page): Promise<{ over: boolean; status: string }> {
  const status = await page.locator("#status").textContent() || "";
  const over = (
    status.includes("Victory") || 
    status.includes("Defeat") ||
    status.includes("You win") ||
    status.includes("All enemy ships sunk") ||
    status.includes("All ships sunk") ||
    status.includes("won") || 
    status.includes("lost") || 
    status.includes("Game ended") ||
    status.includes("surrendered") ||
    status.includes("timed out") ||
    status.includes("cheated") ||
    status.includes("Cheating") ||
    status.includes("invalid ship pattern")
  );
  if (over) {
    console.log(`  [isGameOver] Detected: "${status}"`);
  }
  return { over, status };
}

async function clickEnemyBoard(page: Page, gridX: number, gridY: number) {
  const canvas = page.locator("#enemy-board");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Enemy board not found");
  
  // Isometric grid constants (from types/index.ts)
  const TILE_WIDTH = 64;
  const TILE_HEIGHT = 32;
  const BOARD_OFFSET_X = 320;
  const BOARD_OFFSET_Y = 40;
  
  // gridToScreen conversion (from IsoUtils.ts)
  const screenX = BOARD_OFFSET_X + (gridX - gridY) * (TILE_WIDTH / 2);
  const screenY = BOARD_OFFSET_Y + (gridX + gridY) * (TILE_HEIGHT / 2);
  
  // Click at center of tile (offset by half tile height for isometric center)
  const clickX = box.x + screenX;
  const clickY = box.y + screenY + (TILE_HEIGHT / 2);
  
  await page.mouse.click(clickX, clickY);
}

async function playTurn(page: Page, attackCoords: { x: number; y: number }[]) {
  let coordIndex = 0;
  
  for (let turn = 0; turn < 200; turn++) {
    if (await isGameOver(page)) {
      return;
    }
    
    if (await isOurTurn(page)) {
      const coord = attackCoords[coordIndex % attackCoords.length];
      coordIndex++;
      
      console.log(`  Attacking (${coord.x}, ${coord.y})`);
      await clickEnemyBoard(page, coord.x, coord.y);
      await page.waitForTimeout(2000);
    } else {
      await page.waitForTimeout(1000);
    }
  }
}

async function test() {
  console.log("=".repeat(60));
  console.log("FULL BROWSER E2E TEST");
  console.log("=".repeat(60));

  console.log("Starting smoldot proxy...");
  const proxy = await startProxy(PROXY_PORT);
  console.log(`Smoldot proxy running on port ${proxy.port}`);
  console.log("Waiting 60s for smoldot to sync...");
  await new Promise(r => setTimeout(r, 60000));

  console.log("Launching browser...");
  const browser = await chromium.launch({
    headless: true,
    executablePath: CHROMIUM_PATH,
    args: [
      "--disable-background-timer-throttling",
      "--disable-backgrounding-occluded-windows",
      "--disable-renderer-backgrounding",
    ],
  });
  console.log("Browser launched");

  const aliceContext = await browser.newContext();
  const bobContext = await browser.newContext();

  const alicePage = await aliceContext.newPage();
  const bobPage = await bobContext.newPage();

  alicePage.on("console", (msg) => {
    if (msg.type() === "error") console.log(`[Alice ERROR] ${msg.text()}`);
    console.log(`[Alice] ${msg.text()}`);
  });
  alicePage.on("pageerror", (err) => console.log(`[Alice PAGE ERROR] ${err.message}`));
  bobPage.on("console", (msg) => {
    if (msg.type() === "error") console.log(`[Bob ERROR] ${msg.text()}`);
    console.log(`[Bob] ${msg.text()}`);
  });
  bobPage.on("pageerror", (err) => console.log(`[Bob PAGE ERROR] ${err.message}`));

  try {
    console.log("\n--- PHASE 1: Connect to lobby ---");
    const rpcParam = encodeURIComponent(RPC_URL);
    const proxyParam = encodeURIComponent(`ws://127.0.0.1:${PROXY_PORT}`);
    await alicePage.goto(`${BASE_URL}/?devMode=true&rpc=${rpcParam}&smoldotProxy=${proxyParam}`);
    await bobPage.goto(`${BASE_URL}/?devMode=true&rpc=${rpcParam}&smoldotProxy=${proxyParam}`);

    await selectDevPlayer(alicePage, "alice");
    console.log("✓ Alice connected to lobby");
    
    await selectDevPlayer(bobPage, "bob");
    console.log("✓ Bob connected to lobby");

    console.log("\n--- PHASE 2: Create and join game ---");
    await createGame(alicePage, "1");
    console.log("✓ Alice created game");

    await joinGame(bobPage);
    console.log("✓ Bob joined game");

    console.log("\n--- PHASE 3: Enter game screen ---");
    await Promise.all([
      waitForGameScreen(alicePage),
      waitForGameScreen(bobPage),
    ]);
    console.log("✓ Both players in game screen");

    console.log("\n--- PHASE 4: Setup phase - place ships ---");
    await Promise.all([
      waitForSetupPhase(alicePage),
      waitForSetupPhase(bobPage),
    ]);
    console.log("✓ Both players in setup phase");

    await placeShipsRandomly(alicePage);
    console.log("✓ Alice placed ships randomly");
    
    await placeShipsRandomly(bobPage);
    console.log("✓ Bob placed ships randomly");

    console.log("\n--- PHASE 5: Commit grids ---");
    await Promise.all([
      commitGrid(alicePage),
      commitGrid(bobPage),
    ]);
    console.log("✓ Both players committed grids");

    console.log("\n--- PHASE 6: Battle phase ---");
    await Promise.all([
      waitForBattlePhase(alicePage),
      waitForBattlePhase(bobPage),
    ]);
    console.log("✓ Battle started!");

    // Checkerboard pattern: ships are ≥2 cells, so alternating cells guarantee hits
    const checkerboardCoords: { x: number; y: number }[] = [];
    const remainingCoords: { x: number; y: number }[] = [];
    for (let y = 0; y < 10; y++) {
      for (let x = 0; x < 10; x++) {
        if ((x + y) % 2 === 0) {
          checkerboardCoords.push({ x, y });
        } else {
          remainingCoords.push({ x, y });
        }
      }
    }
    const allCoords = [...checkerboardCoords, ...remainingCoords];

    console.log("\n--- PHASE 7: Play battle (this may take a while) ---");

    let aliceCoordIdx = 0;
    let bobCoordIdx = 0;
    let gameOver = false;
    let aliceFinalStatus = "";
    let bobFinalStatus = "";
    const battleStart = Date.now();
    const BATTLE_TIMEOUT_MS = 900000; // 15 minutes

    async function waitForTurnChange(pg: Page, label: string): Promise<boolean> {
      for (let w = 0; w < 80; w++) {
        await pg.waitForTimeout(250);
        const s = await pg.locator("#status").textContent() || "";
        if (s.includes("Opponent's turn") || s.includes("Waiting for") ||
            s.includes("Victory") || s.includes("Defeat") ||
            s.includes("All") || s.includes("won") || s.includes("lost") ||
            s.includes("Game ended") || s.includes("surrendered") ||
            s.includes("cheated") || s.includes("Cheating") || s.includes("invalid ship")) {
          return true;
        }
      }
      console.log(`  ${label}: turn change not detected in 20s`);
      return false;
    }

    for (let round = 0; round < 40000 && !gameOver; round++) {
      if (Date.now() - battleStart > BATTLE_TIMEOUT_MS) {
        throw new Error(`Battle timed out after ${BATTLE_TIMEOUT_MS / 1000}s`);
      }

      await alicePage.waitForTimeout(250);

      const aliceStatus = await alicePage.locator("#status").textContent() || "";
      const bobStatus = await bobPage.locator("#status").textContent() || "";

      if (round % 40 === 0) {
        const elapsed = ((Date.now() - battleStart) / 1000).toFixed(0);
        console.log(`  [${elapsed}s] Alice="${aliceStatus.slice(0,50)}" Bob="${bobStatus.slice(0,50)}" (A=${aliceCoordIdx}, B=${bobCoordIdx})`);
      }

      const aliceGameOver = await isGameOver(alicePage);
      const bobGameOver = await isGameOver(bobPage);
      if (aliceGameOver.over || bobGameOver.over) {
        let aliceFinal = aliceGameOver;
        let bobFinal = bobGameOver;

        if (!aliceGameOver.over || !bobGameOver.over) {
          for (let i = 0; i < 60; i++) {
            await alicePage.waitForTimeout(1000);
            if (!aliceFinal.over) aliceFinal = await isGameOver(alicePage);
            if (!bobFinal.over) bobFinal = await isGameOver(bobPage);
            if (aliceFinal.over && bobFinal.over) break;
          }
        }

        console.log(`  Game ended: Alice="${aliceFinal.status}", Bob="${bobFinal.status}"`);
        aliceFinalStatus = aliceFinal.status;
        bobFinalStatus = bobFinal.status;
        gameOver = true;
        break;
      }

      const aliceCanAttack = aliceStatus.includes("Your turn") && !aliceStatus.includes("failed");
      const bobCanAttack = bobStatus.includes("Your turn") && !bobStatus.includes("failed");

      if (aliceCanAttack && aliceCoordIdx < 100) {
        const coord = allCoords[aliceCoordIdx];
        console.log(`  Alice attacks (${coord.x}, ${coord.y})`);
        await clickEnemyBoard(alicePage, coord.x, coord.y);
        aliceCoordIdx++;
        await waitForTurnChange(alicePage, "Alice");
      } else if (bobCanAttack && bobCoordIdx < 100) {
        const coord = allCoords[bobCoordIdx];
        console.log(`  Bob attacks (${coord.x}, ${coord.y})`);
        await clickEnemyBoard(bobPage, coord.x, coord.y);
        bobCoordIdx++;
        await waitForTurnChange(bobPage, "Bob");
      }
    }
    
    console.log("\n--- PHASE 8: Verify game ended ---");
    
    const totalAttacks = aliceCoordIdx + bobCoordIdx;
    console.log(`  Total attacks made: ${totalAttacks} (Alice: ${aliceCoordIdx}, Bob: ${bobCoordIdx})`);
    
    const aliceStatus = aliceFinalStatus || await alicePage.locator("#status").textContent() || "";
    const bobStatus = bobFinalStatus || await bobPage.locator("#status").textContent() || "";
    console.log(`  Alice final status: "${aliceStatus}"`);
    console.log(`  Bob final status: "${bobStatus}"`);

    const MIN_ATTACKS_FOR_WIN = 17;
    if (totalAttacks < MIN_ATTACKS_FOR_WIN) {
      throw new Error(`Game ended too early! Only ${totalAttacks} attacks made, need at least ${MIN_ATTACKS_FOR_WIN} to sink all ships.`);
    }

    const validWinPatterns = ["Victory", "All enemy ships sunk"];
    const validLosePatterns = ["Defeat", "All ships sunk"];
    
    const aliceWon = validWinPatterns.some(p => aliceStatus.includes(p));
    const aliceLost = validLosePatterns.some(p => aliceStatus.includes(p));
    const bobWon = validWinPatterns.some(p => bobStatus.includes(p));
    const bobLost = validLosePatterns.some(p => bobStatus.includes(p));

    console.log(`  Alice: won=${aliceWon}, lost=${aliceLost}`);
    console.log(`  Bob: won=${bobWon}, lost=${bobLost}`);

    if (aliceStatus.includes("cheated") || aliceStatus.includes("Cheating") || 
        bobStatus.includes("cheated") || bobStatus.includes("Cheating") ||
        aliceStatus.includes("invalid ship") || bobStatus.includes("invalid ship")) {
      throw new Error(`Game ended with cheating detection (InvalidHitPattern or similar). Alice: "${aliceStatus}", Bob: "${bobStatus}"`);
    }

    if (aliceStatus.includes("surrendered") || bobStatus.includes("surrendered") ||
        aliceStatus.includes("timed out") || bobStatus.includes("timed out")) {
      throw new Error(`Game ended with surrender/timeout, not AllShipsSunk. Alice: "${aliceStatus}", Bob: "${bobStatus}"`);
    }

    if (aliceStatus.includes("unexpectedly") || bobStatus.includes("unexpectedly")) {
      throw new Error(`Game ended unexpectedly. Alice: "${aliceStatus}", Bob: "${bobStatus}"`);
    }

    if (!((aliceWon && bobLost) || (bobWon && aliceLost))) {
      throw new Error(`Invalid game outcome. Expected AllShipsSunk win/loss. Alice: "${aliceStatus}", Bob: "${bobStatus}"`);
    }

    const winner = aliceWon ? "Alice" : "Bob";
    console.log(`  Winner: ${winner}`);

    console.log("\n" + "=".repeat(60));
    console.log("FULL BROWSER E2E TEST PASSED!");
    console.log("=".repeat(60));

  } catch (error) {
    console.error("\nTest failed:", error);
    
    const aliceScreenshot = await alicePage.screenshot().catch(() => null);
    const bobScreenshot = await bobPage.screenshot().catch(() => null);
    
    if (aliceScreenshot) {
      require("fs").writeFileSync("alice-failure.png", aliceScreenshot);
      console.log("  Saved alice-failure.png");
    }
    if (bobScreenshot) {
      require("fs").writeFileSync("bob-failure.png", bobScreenshot);
      console.log("  Saved bob-failure.png");
    }
    
    const aliceContent = await alicePage.locator("#status").textContent().catch(() => "N/A");
    const bobContent = await bobPage.locator("#status").textContent().catch(() => "N/A");
    console.log("  Alice status:", aliceContent);
    console.log("  Bob status:", bobContent);
    
    throw error;
  } finally {
    await browser.close();
    proxy.stop();
  }
}

test()
  .then(() => process.exit(0))
  .catch(() => process.exit(1));
