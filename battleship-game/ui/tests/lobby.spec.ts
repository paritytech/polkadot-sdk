import { test, expect, Page } from "@playwright/test";

async function enterUsernameAndWaitForLobby(page: Page, name: string, timeoutMs = 180_000) {
  const input = page.locator("#username-input");
  await expect(input).toBeVisible({ timeout: 10_000 });
  await input.fill(name);
  await page.click("#username-confirm-btn");
  await expect(page.locator("#game-lobby.screen.active")).toBeVisible({ timeout: timeoutMs });
}

test.describe("Game Lobby", () => {
  test.setTimeout(300_000);

  test("Alice creates game, Bob sees it in lobby", async ({ browser }) => {
    const aliceContext = await browser.newContext();
    const bobContext = await browser.newContext();

    const alicePage = await aliceContext.newPage();
    const bobPage = await bobContext.newPage();

    alicePage.on("console", (msg) => console.log(`[Alice] ${msg.text()}`));
    bobPage.on("console", (msg) => console.log(`[Bob] ${msg.text()}`));

    await alicePage.goto("/");
    await bobPage.goto("/");

    await enterUsernameAndWaitForLobby(alicePage, "Alice");
    await enterUsernameAndWaitForLobby(bobPage, "Bob");

    await alicePage.click("#create-game-btn");
    await expect(alicePage.locator("#fund-modal")).toHaveClass(/active/);

    await alicePage.fill("#pot-amount-input", "1");
    await alicePage.click("#confirm-fund-btn");

    await expect(alicePage.getByText("Waiting for opponent to join...")).toBeVisible({ timeout: 30_000 });

    await bobPage.waitForTimeout(6000);
    await bobPage.click("#refresh-lobby-btn");

    const gameCard = bobPage.locator(".game-card").first();
    await expect(gameCard).toBeVisible({ timeout: 30_000 });
    await expect(gameCard.getByText("Stake: 1.000 UNIT")).toBeVisible();

    await aliceContext.close();
    await bobContext.close();
  });

  test("Alice creates before Bob reaches lobby, Bob still sees game later", async ({ browser }) => {
    const aliceContext = await browser.newContext();
    const bobContext = await browser.newContext();

    const alicePage = await aliceContext.newPage();
    const bobPage = await bobContext.newPage();

    alicePage.on("console", (msg) => console.log(`[Alice] ${msg.text()}`));
    bobPage.on("console", (msg) => console.log(`[Bob] ${msg.text()}`));

    await alicePage.goto("/");
    await bobPage.goto("/?testDelayBeforeLobbyMs=10000");

    const bobInput = bobPage.locator("#username-input");
    await expect(bobInput).toBeVisible({ timeout: 10_000 });
    await bobInput.fill("Bob");
    await bobPage.click("#username-confirm-btn");

    await enterUsernameAndWaitForLobby(alicePage, "Alice");

    await alicePage.click("#create-game-btn");
    await expect(alicePage.locator("#fund-modal")).toHaveClass(/active/);
    await alicePage.fill("#pot-amount-input", "1");
    await alicePage.click("#confirm-fund-btn");
    await expect(alicePage.getByText("Waiting for opponent to join...")).toBeVisible({ timeout: 30_000 });

    await expect(bobPage.locator("#game-lobby.screen.active")).toBeVisible({ timeout: 180_000 });

    await expect(async () => {
      await bobPage.click("#refresh-lobby-btn");
      const gameCard = bobPage.locator(".game-card").first();
      await expect(gameCard).toBeVisible({ timeout: 5000 });
      await expect(gameCard.getByText("Stake: 1.000 UNIT")).toBeVisible({ timeout: 5000 });
    }).toPass({ timeout: 60_000, intervals: [5000] });

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

    await alicePage.goto("/");
    await bobPage.goto("/");

    await enterUsernameAndWaitForLobby(alicePage, "Alice");
    await enterUsernameAndWaitForLobby(bobPage, "Bob");

    await alicePage.click("#create-game-btn");
    await alicePage.fill("#pot-amount-input", "1");
    await alicePage.click("#confirm-fund-btn");

    await expect(alicePage.getByText("Waiting for opponent to join...")).toBeVisible({ timeout: 30_000 });

    await bobPage.waitForTimeout(6000);
    await bobPage.click("#refresh-lobby-btn");

    const gameCard = bobPage.locator(".game-card").first();
    await expect(gameCard).toBeVisible({ timeout: 30_000 });

    await gameCard.locator("button", { hasText: "Join Game" }).click();

    await expect(alicePage.locator("#game")).toBeVisible({ timeout: 120_000 });
    await expect(bobPage.locator("#game")).toBeVisible({ timeout: 120_000 });

    await aliceContext.close();
    await bobContext.close();
  });
});
