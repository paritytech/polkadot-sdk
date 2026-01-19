import { test, expect, Page } from "@playwright/test";

async function selectDevPlayer(page: Page, player: "alice" | "bob") {
  await page.click(`[data-player="${player}"]`);
  await page.click("#wallet-continue-btn");
  await expect(page.locator(".screen.active#game-lobby")).toBeVisible();
}

async function waitForText(page: Page, text: string, timeout = 30000) {
  await expect(page.getByText(text)).toBeVisible({ timeout });
}

test.describe("Game Lobby", () => {
  test("Alice creates game, Bob sees it in lobby", async ({ browser }) => {
    const aliceContext = await browser.newContext();
    const bobContext = await browser.newContext();

    const alicePage = await aliceContext.newPage();
    const bobPage = await bobContext.newPage();

    await alicePage.goto("/?devMode=true");
    await bobPage.goto("/?devMode=true");

    await selectDevPlayer(alicePage, "alice");
    await selectDevPlayer(bobPage, "bob");

    await alicePage.click("#create-game-btn");
    await expect(alicePage.locator("#fund-modal")).toHaveClass(/active/);

    await alicePage.fill("#pot-amount-input", "1");
    await alicePage.click("#confirm-fund-btn");

    await waitForText(alicePage, "Waiting for opponent to join...");

    await bobPage.waitForTimeout(4000);
    await bobPage.click("#refresh-lobby-btn");

    const gameCard = bobPage.locator(".game-card").first();
    await expect(gameCard).toBeVisible({ timeout: 10000 });
    await expect(gameCard.getByText("Stake: 1.0 UNIT")).toBeVisible();

    await aliceContext.close();
    await bobContext.close();
  });

  test("Full flow: Alice creates, Bob joins, both enter game", async ({
    browser,
  }) => {
    const aliceContext = await browser.newContext();
    const bobContext = await browser.newContext();

    const alicePage = await aliceContext.newPage();
    const bobPage = await bobContext.newPage();

    alicePage.on("console", (msg) => console.log(`[Alice] ${msg.text()}`));
    bobPage.on("console", (msg) => console.log(`[Bob] ${msg.text()}`));

    await alicePage.goto("/?devMode=true");
    await bobPage.goto("/?devMode=true");

    await selectDevPlayer(alicePage, "alice");
    await selectDevPlayer(bobPage, "bob");

    await alicePage.click("#create-game-btn");
    await alicePage.fill("#pot-amount-input", "1");
    await alicePage.click("#confirm-fund-btn");

    await waitForText(alicePage, "Waiting for opponent to join...");

    await bobPage.waitForTimeout(4000);
    await bobPage.click("#refresh-lobby-btn");

    const gameCard = bobPage.locator(".game-card").first();
    await expect(gameCard).toBeVisible({ timeout: 10000 });

    await gameCard.locator("button", { hasText: "Join Game" }).click();

    await waitForText(bobPage, "Waiting for host to create game...");

    await waitForText(alicePage, "Opponent found! Creating game...", 30000);

    await expect(alicePage.locator("#game")).toBeVisible({ timeout: 60000 });
    await expect(bobPage.locator("#game")).toBeVisible({ timeout: 60000 });

    await aliceContext.close();
    await bobContext.close();
  });
});
