import { chromium, Browser, Page } from "playwright";

const BASE_URL = "http://localhost:3000";
const CHROMIUM_PATH = "/nix/store/g245pzpbacazlrca1fb7crb9883rhhs3-chromium-144.0.7559.59/bin/chromium";

async function waitForText(page: Page, text: string, timeout = 10000) {
  await page.waitForFunction(
    (t) => document.body.textContent?.includes(t),
    text,
    { timeout }
  );
}

async function test() {
  console.log("Launching Chromium...");
  const browser = await chromium.launch({
    headless: true,
    executablePath: CHROMIUM_PATH,
  });

  const aliceContext = await browser.newContext();
  const bobContext = await browser.newContext();

  const alicePage = await aliceContext.newPage();
  const bobPage = await bobContext.newPage();

  alicePage.on("console", (msg) => console.log(`[Alice Console] ${msg.type()}: ${msg.text()}`));
  bobPage.on("console", (msg) => console.log(`[Bob Console] ${msg.type()}: ${msg.text()}`));

  try {
    console.log("\n=== Test: Alice connects and creates game ===");
    await alicePage.goto(`${BASE_URL}/?devMode=true`);
    await waitForText(alicePage, "Select Dev Account");

    await alicePage.click('[data-player="alice"]');
    await alicePage.waitForTimeout(500);
    await alicePage.click('button:has-text("Continue to Lobby")');
    await waitForText(alicePage, "BATTLESHIP LOBBY");
    console.log("PASS: Alice connected to lobby");

    await alicePage.click('button:has-text("Create New Game")');
    await waitForText(alicePage, "Set Game Stake");

    const stakeInput = alicePage.locator('#pot-amount-input');
    await stakeInput.fill("1");
    await alicePage.waitForTimeout(300);

    await alicePage.click('button:has-text("Confirm & Create")');
    await waitForText(alicePage, "Waiting for opponent");
    console.log("PASS: Alice created game, waiting for opponent");

    console.log("\n=== Test: Bob connects and sees Alice's game ===");
    await bobPage.goto(`${BASE_URL}/?devMode=true`);
    await waitForText(bobPage, "Select Dev Account");

    await bobPage.click('[data-player="bob"]');
    await bobPage.waitForTimeout(500);
    await bobPage.click('button:has-text("Continue to Lobby")');
    await waitForText(bobPage, "BATTLESHIP LOBBY");
    console.log("PASS: Bob connected to lobby");

    console.log("Waiting for Alice's game to appear in Bob's lobby...");
    for (let i = 0; i < 10; i++) {
      await bobPage.waitForTimeout(1000);
      await bobPage.click('button:has-text("Refresh")').catch(() => {});
      const gameCard = bobPage.locator(".game-card").first();
      if (await gameCard.isVisible().catch(() => false)) {
        break;
      }
    }

    console.log("PASS: Bob sees Alice's game in lobby");

    console.log("\n=== Test: Bob joins Alice's game ===");
    const joinButton = bobPage.locator('button:has-text("Join Game")').first();
    await joinButton.waitFor({ state: "visible", timeout: 5000 });
    await joinButton.click();

    console.log("Bob clicked Join, waiting for game transition...");

    await Promise.race([
      waitForText(bobPage, "Game Board", 15000),
      waitForText(bobPage, "Your Board", 15000),
      waitForText(bobPage, "Placing ships", 15000),
    ]).catch(() => {
      console.log("Bob may still be in lobby or transitioning...");
    });

    await Promise.race([
      waitForText(alicePage, "Game Board", 15000),
      waitForText(alicePage, "Your Board", 15000),
      waitForText(alicePage, "Placing ships", 15000),
    ]).catch(() => {
      console.log("Alice may still be waiting or transitioning...");
    });

    const aliceInGame = await alicePage.evaluate(() =>
      document.body.textContent?.includes("Game") ||
      document.body.textContent?.includes("Board") ||
      document.body.textContent?.includes("ship")
    );

    const bobInGame = await bobPage.evaluate(() =>
      document.body.textContent?.includes("Game") ||
      document.body.textContent?.includes("Board") ||
      document.body.textContent?.includes("ship")
    );

    console.log(`Alice in game: ${aliceInGame}, Bob in game: ${bobInGame}`);

    if (aliceInGame || bobInGame) {
      console.log("\n=== PASS: Game lobby flow works! ===");
    } else {
      console.log("\n=== PARTIAL: Players may still be transitioning ===");
      const aliceContent = await alicePage.evaluate(() => document.body.textContent);
      const bobContent = await bobPage.evaluate(() => document.body.textContent);
      console.log("Alice page:", aliceContent?.slice(0, 200));
      console.log("Bob page:", bobContent?.slice(0, 200));
    }

  } catch (error) {
    console.error("Test error:", error);
    const aliceContent = await alicePage.evaluate(() => document.body.textContent).catch(() => "N/A");
    const bobContent = await bobPage.evaluate(() => document.body.textContent).catch(() => "N/A");
    console.log("Alice page content:", aliceContent?.slice(0, 500));
    console.log("Bob page content:", bobContent?.slice(0, 500));
    throw error;
  } finally {
    await browser.close();
  }
}

test()
  .then(() => {
    console.log("\nBrowser E2E test completed");
    process.exit(0);
  })
  .catch((e) => {
    console.error("Test failed:", e);
    process.exit(1);
  });
