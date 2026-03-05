import { chromium, type Page } from "playwright";

const BASE_URL = "http://localhost:3000";
const CHROMIUM_PATH = "/nix/store/hwjzmx82i8dzm2c5l7hyj8yd62222ifq-chromium-145.0.7632.109/bin/chromium";

async function testTransaction() {
  const browser = await chromium.launch({
    headless: true,
    executablePath: CHROMIUM_PATH,
  });

  const context = await browser.newContext();
  const page = await context.newPage();

  const logs: string[] = [];
  page.on("console", (msg) => {
    const text = `[${msg.type()}] ${msg.text()}`;
    logs.push(text);
    console.log(text);
  });

  try {
    console.log("Navigating to app...");
    await page.goto(`${BASE_URL}/?devMode=true`);
    
    console.log("Waiting for player selection...");
    await page.waitForSelector("[data-player]", { timeout: 30000 });
    
    console.log("Selecting Alice...");
    await page.click('[data-player="alice"]');
    await page.click("#wallet-continue-btn");
    
    console.log("Waiting for lobby (180s)...");
    for (let i = 0; i < 180; i++) {
      const lobby = await page.locator("#game-lobby.active").isVisible().catch(() => false);
      if (lobby) {
        console.log(`✓ Lobby visible after ${i}s`);
        break;
      }
      if (i % 10 === 0) console.log(`  Still waiting... ${i}s`);
      await page.waitForTimeout(1000);
    }
    
    console.log("\nWaiting 30s more for sync...");
    await page.waitForTimeout(30000);
    
    console.log("\nAttempting to create game...");
    const createBtn = page.locator("#create-game-btn");
    const isVisible = await createBtn.isVisible().catch(() => false);
    console.log(`Create button visible: ${isVisible}`);
    
    if (isVisible) {
      await createBtn.click();
      console.log("Clicked create game, waiting for modal...");
      
      await page.waitForSelector("#fund-modal.active", { timeout: 30000 }).catch(e => {
        console.log("Modal timeout:", e.message);
      });
      
      console.log("Waiting 60s for transaction to land...");
      for (let i = 0; i < 60; i++) {
        const text = await page.textContent("body");
        if (text?.includes("Waiting for opponent")) {
          console.log(`✓ Transaction landed after ${i}s!`);
          break;
        }
        if (i % 10 === 0) console.log(`  Still waiting... ${i}s`);
        await page.waitForTimeout(1000);
      }
    }
    
  } catch (e) {
    console.error("Error:", e);
  } finally {
    console.log("\n=== FINAL LOGS ===");
    logs.slice(-50).forEach(l => console.log(l));
    await browser.close();
  }
}

testTransaction().catch(console.error);
