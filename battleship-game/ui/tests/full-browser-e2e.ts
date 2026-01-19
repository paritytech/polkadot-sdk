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
  for (let i = 0; i < 15; i++) {
    await page.waitForTimeout(1000);
    await page.click("#refresh-lobby-btn").catch(() => {});
    const gameCard = page.locator(".game-card").first();
    if (await gameCard.isVisible().catch(() => false)) {
      const joinBtn = gameCard.locator("button", { hasText: "Join Game" });
      if (await joinBtn.isVisible().catch(() => false)) {
        await joinBtn.click();
        return true;
      }
    }
  }
  throw new Error("Could not find game to join");
}

async function waitForGameScreen(page: Page) {
  await waitForSelector(page, "#game.active", 60000);
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

async function isGameOver(page: Page): Promise<boolean> {
  const status = await page.locator("#status").textContent();
  return (status?.includes("won") || status?.includes("lost") || status?.includes("Victory") || status?.includes("Defeat")) || false;
}

async function clickEnemyBoard(page: Page, x: number, y: number) {
  const canvas = page.locator("#enemy-board");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Enemy board not found");
  
  const cellSize = 35;
  const boardOffsetX = 50;
  const boardOffsetY = 50;
  
  const clickX = box.x + boardOffsetX + (x * cellSize) + (cellSize / 2);
  const clickY = box.y + boardOffsetY + (y * cellSize) + (cellSize / 2);
  
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
    
    const playGame = async (page: Page, name: string): Promise<string> => {
      let turnCount = 0;
      let coordIndex = 0;
      
      while (turnCount < 200) {
        try {
          if (await isGameOver(page)) {
            const status = await page.locator("#status").textContent();
            console.log(`  ${name}: Game over - ${status}`);
            return status || "unknown";
          }
          
          if (await isOurTurn(page)) {
            const coord = allCoords[coordIndex];
            coordIndex++;
            turnCount++;
            
            if (turnCount % 5 === 0) {
              console.log(`  ${name}: Turn ${turnCount}, attacking (${coord.x}, ${coord.y})`);
            }
            
            await clickEnemyBoard(page, coord.x, coord.y);
            await page.waitForTimeout(3000);
          } else {
            await page.waitForTimeout(500);
          }
        } catch (e: any) {
          if (e.message?.includes("closed") || e.message?.includes("Target")) {
            const status = await page.locator("#status").textContent().catch(() => "page closed");
            console.log(`  ${name}: Page closed or navigated - ${status}`);
            return status || "page closed";
          }
          throw e;
        }
      }
      return "timeout";
    };

    const results = await Promise.race([
      Promise.all([
        playGame(alicePage, "Alice"),
        playGame(bobPage, "Bob"),
      ]),
      new Promise<string[]>((_, reject) => setTimeout(() => reject(new Error("Game timeout after 10 minutes")), 600000)),
    ]);

    console.log("\n--- PHASE 8: Verify game ended ---");
    
    const [aliceResult, bobResult] = results;
    console.log(`  Alice result: ${aliceResult}`);
    console.log(`  Bob result: ${bobResult}`);

    const gameEnded = 
      (aliceResult?.includes("won") || aliceResult?.includes("lost") || aliceResult?.includes("Victory") || aliceResult?.includes("Defeat") ||
       bobResult?.includes("won") || bobResult?.includes("lost") || bobResult?.includes("Victory") || bobResult?.includes("Defeat"));

    if (!gameEnded) {
      throw new Error("Game did not reach completion");
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
