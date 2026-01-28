import { chromium, Page } from "playwright";

const BASE_URL = "http://localhost:3000";
const CHROMIUM_PATH = "/nix/store/g245pzpbacazlrca1fb7crb9883rhhs3-chromium-144.0.7559.59/bin/chromium";

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
  
  await page.waitForTimeout(3000);
  
  const inGameScreen = await page.locator("#game.active").isVisible().catch(() => false);
  if (inGameScreen) {
    console.log(`  ${player} has existing game, checking if playable...`);
    const statusText = await page.locator("#status").textContent().catch(() => "");
    
    if (statusText?.includes("Cannot reveal") || statusText?.includes("ERROR") || statusText?.includes("surrender")) {
      console.log(`  ${player} has stale game, surrendering...`);
      const surrenderBtn = page.locator("#surrender-btn");
      for (let i = 0; i < 10; i++) {
        if (await surrenderBtn.isEnabled().catch(() => false)) {
          await surrenderBtn.click();
          await page.waitForTimeout(5000);
          break;
        }
        await page.waitForTimeout(500);
      }
    } else {
      console.log(`  ${player} resuming active game, surrendering to start fresh...`);
      const surrenderBtn = page.locator("#surrender-btn");
      for (let i = 0; i < 20; i++) {
        if (await surrenderBtn.isEnabled().catch(() => false)) {
          await surrenderBtn.click();
          await page.waitForTimeout(5000);
          break;
        }
        await page.waitForTimeout(500);
      }
    }
    
    await page.goto(`${BASE_URL}/?devMode=true`);
    await page.click(`[data-player="${player}"]`);
    await page.click("#wallet-continue-btn");
  }
  
  await waitForText(page, "BATTLESHIP LOBBY");
}

async function createGame(page: Page, stake: string = "1") {
  await page.click("#create-game-btn");
  await waitForSelector(page, "#fund-modal.active");
  await page.fill("#pot-amount-input", stake);
  await page.waitForTimeout(300);
  await page.click("#confirm-fund-btn");
  await waitForText(page, "Waiting for opponent", 30000);
}

async function joinGame(page: Page) {
  for (let i = 0; i < 30; i++) {
    await page.waitForTimeout(1000);
    await page.click("#refresh-lobby-btn").catch(() => {});
    await page.waitForTimeout(500);
    const gameCard = page.locator(".game-card").first();
    if (await gameCard.isVisible().catch(() => false)) {
      const joinBtn = gameCard.locator("button", { hasText: "Join Game" });
      if (await joinBtn.isVisible().catch(() => false)) {
        await joinBtn.click();
        await page.waitForTimeout(3000);
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
    
    const inLobby = await page.locator("#lobby.active").isVisible().catch(() => false);
    if (inLobby) {
      const gameCards = await page.locator(".game-card").count().catch(() => 0);
      console.log(`  Still in lobby, ${gameCards} games visible, waiting...`);
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
    { timeout: 60000 }
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
      return text.includes("Your turn") || text.includes("Opponent's turn") || text.includes("Waiting");
    },
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
    status.includes("won") || 
    status.includes("lost") || 
    status.includes("Victory!") || 
    status.includes("Defeat!") ||
    status.includes("Game ended") ||
    status.includes("You win!") ||
    status.includes("surrendered") ||
    status.includes("timed out!")
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

  console.log("Launching browser...");
  const browser = await chromium.launch({
    headless: true,
    executablePath: CHROMIUM_PATH,
  });
  console.log("Browser launched");

  const aliceContext = await browser.newContext();
  const bobContext = await browser.newContext();

  const alicePage = await aliceContext.newPage();
  const bobPage = await bobContext.newPage();

  alicePage.on("console", (msg) => {
    if (msg.type() === "error") console.log(`[Alice ERROR] ${msg.text()}`);
  });
  bobPage.on("console", (msg) => {
    if (msg.type() === "error") console.log(`[Bob ERROR] ${msg.text()}`);
  });

  try {
    console.log("\n--- PHASE 1: Connect to lobby ---");
    await alicePage.goto(`${BASE_URL}/?devMode=true`);
    await bobPage.goto(`${BASE_URL}/?devMode=true`);

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

    const allCoords: { x: number; y: number }[] = [];
    for (let y = 0; y < 10; y++) {
      for (let x = 0; x < 10; x++) {
        allCoords.push({ x, y });
      }
    }

    console.log("\n--- PHASE 7: Play battle (this may take a while) ---");
    
    let aliceCoordIdx = 0;
    let bobCoordIdx = 0;
    let gameOver = false;
    let finalStatus = "";
    
    let lastAliceAttackIdx = -1;
    let lastBobAttackIdx = -1;
    
    for (let round = 0; round < 2000 && !gameOver; round++) {
      await alicePage.waitForTimeout(200);
      
      const aliceStatus = await alicePage.locator("#status").textContent() || "";
      const bobStatus = await bobPage.locator("#status").textContent() || "";
      
      if (round % 100 === 0) {
        console.log(`  Round ${round}: Alice="${aliceStatus.slice(0,30)}" Bob="${bobStatus.slice(0,30)}" (attacks: A=${aliceCoordIdx}, B=${bobCoordIdx})`);
      }
      
      const aliceGameOver = await isGameOver(alicePage);
      const bobGameOver = await isGameOver(bobPage);
      if (aliceGameOver.over || bobGameOver.over) {
        finalStatus = aliceGameOver.over ? aliceGameOver.status : bobGameOver.status;
        console.log(`  Game ended: ${finalStatus}`);
        gameOver = true;
        break;
      }
      
      const aliceCanAttack = aliceStatus.includes("Your turn") && !aliceStatus.includes("failed");
      const bobCanAttack = bobStatus.includes("Your turn") && !bobStatus.includes("failed");
      
      if (aliceCanAttack && aliceCoordIdx !== lastAliceAttackIdx && aliceCoordIdx < 100) {
        const coord = allCoords[aliceCoordIdx];
        console.log(`  Alice attacks (${coord.x}, ${coord.y})`);
        await clickEnemyBoard(alicePage, coord.x, coord.y);
        lastAliceAttackIdx = aliceCoordIdx;
        aliceCoordIdx++;
        await alicePage.waitForTimeout(1000);
      } else if (bobCanAttack && bobCoordIdx !== lastBobAttackIdx && bobCoordIdx < 100) {
        const coord = allCoords[bobCoordIdx];
        console.log(`  Bob attacks (${coord.x}, ${coord.y})`);
        await clickEnemyBoard(bobPage, coord.x, coord.y);
        lastBobAttackIdx = bobCoordIdx;
        bobCoordIdx++;
        await bobPage.waitForTimeout(1000);
      }
    }
    
    const results = [finalStatus, finalStatus];

    console.log("\n--- PHASE 8: Verify game ended ---");
    
    const [aliceResult, bobResult] = results;
    console.log(`  Alice result: ${aliceResult}`);
    console.log(`  Bob result: ${bobResult}`);

    const endPatterns = ["won", "lost", "Victory", "Defeat", "Game ended", "You win", "surrendered", "timed out"];
    const gameEnded = endPatterns.some(p => 
      aliceResult?.includes(p) || bobResult?.includes(p)
    );

    if (!gameEnded) {
      throw new Error(`Game did not reach completion. Alice: "${aliceResult}", Bob: "${bobResult}"`);
    }

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
  }
}

test()
  .then(() => process.exit(0))
  .catch(() => process.exit(1));
