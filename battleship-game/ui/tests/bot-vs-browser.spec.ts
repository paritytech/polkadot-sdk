import { test, expect } from "@playwright/test";
import { spawn, ChildProcess } from "child_process";
import path from "path";
import { fileURLToPath } from "url";

// The bot runs as a separate Node.js process with its own smoldot instance.
// The browser runs the UI with its own smoldot instance.
// Game discovery goes through statement store ping/pong.

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const BOT_DIR = path.resolve(__dirname, "../../bot");

// Total ship cells: Carrier(5) + Battleship(4) + Cruiser(3) + Submarine(3) + Destroyer(2) = 17
const TOTAL_SHIP_CELLS = 17;

function startBot(): ChildProcess {
	const bot = spawn("node", ["dist/index.js"], {
		cwd: BOT_DIR,
		stdio: ["pipe", "pipe", "pipe"],
		env: { ...process.env, NODE_NO_WARNINGS: "1" },
	});

	bot.stdout?.on("data", (data: Buffer) => {
		for (const line of data.toString().split("\n").filter(Boolean)) {
			console.log(`[BOT] ${line}`);
		}
	});

	bot.stderr?.on("data", (data: Buffer) => {
		for (const line of data.toString().split("\n").filter(Boolean)) {
			if (line.includes("[smoldot:TRACE:") || line.includes("[smoldot:DEBUG:")) return;
			console.log(`[BOT:err] ${line}`);
		}
	});

	return bot;
}

// Isometric grid coordinate conversion matching the UI renderer.
// BOARD_OFFSET_X=320, BOARD_OFFSET_Y=40, TILE_WIDTH=64, TILE_HEIGHT=32.
function gridToScreen(gx: number, gy: number): { x: number; y: number } {
	return {
		x: 320 + (gx - gy) * 32,
		y: 40 + (gx + gy) * 16 + 16, // +16 to hit diamond centre
	};
}

test.describe("Bot vs Browser", () => {
	let bot: ChildProcess;

	test.afterEach(async () => {
		if (bot && !bot.killed) {
			bot.kill("SIGTERM");
			await new Promise((r) => setTimeout(r, 1000));
			if (!bot.killed) bot.kill("SIGKILL");
		}
	});

	for (const gameNum of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]) {
	test(`game ${gameNum}: browser player discovers bot game and plays to completion`, async ({ page }) => {
		test.setTimeout(3_600_000);

		// Start the bot process
		console.log("[Test] Starting bot process...");
		bot = startBot();

		// Navigate to the UI
		console.log("[Test] Opening browser UI...");
		page.on("console", (msg) => {
			console.log(`[Browser] ${msg.text()}`);
		});

		await page.goto("/");

		// Current app flow starts on a username gate before chain connection and lobby setup.
		const usernameInput = page.locator("#username-input");
		if (await usernameInput.isVisible({ timeout: 10_000 }).catch(() => false)) {
			await usernameInput.fill("Browser Player");
			await page.locator("#username-confirm-btn").click();
			console.log("[Test] Submitted username");
		}

		// Wait for lobby
		console.log("[Test] Waiting for lobby...");
		await expect(page.locator("#game-lobby.active")).toBeVisible({ timeout: 240_000 });
		console.log("[Test] Lobby visible!");

		// Wait for balance
		await expect(async () => {
			const balance = await page.locator("#lobby-balance").textContent();
			expect(balance).not.toBe("0.000 UNIT");
		}).toPass({ timeout: 60_000, intervals: [2000] });
		console.log("[Test] Funds received!");

		// Wait for bot's game to appear
		console.log("[Test] Waiting for bot's game to appear in lobby...");
		const gameCard = page.locator(".game-card").first();
		await expect(async () => {
			await page.click("#refresh-lobby-btn");
			await expect(gameCard).toBeVisible({ timeout: 5000 });
		}).toPass({ timeout: 300_000, intervals: [3000] });
		console.log("[Test] Bot's game found in lobby!");

		// Join the game
		const joinBtn = gameCard.locator("button", { hasText: "Join Game" });
		await expect(joinBtn).toBeVisible();
		await joinBtn.click();
		console.log("[Test] Clicked Join Game");

		// Wait for game screen
		await expect(page.locator("#game.active")).toBeVisible({ timeout: 240_000 });
		console.log("[Test] Game screen visible!");

		// Setup phase: place ships randomly and commit
		const randomBtn = page.locator("#random-btn");
		const commitBtn = page.locator("#commit-btn");

		await expect(randomBtn).toBeEnabled({ timeout: 10_000 });
		await randomBtn.click();
		console.log("[Test] Ships placed randomly");

		await expect(commitBtn).toBeEnabled({ timeout: 5_000 });
		await commitBtn.click();
		console.log("[Test] Grid committed");

		// Wait for battle phase
		console.log("[Test] Waiting for battle phase...");
		await expect(async () => {
			const instructions = await page.locator("#instructions").textContent();
			const isBattle =
				instructions?.includes("click enemy waters") ||
				instructions?.includes("Opponent's turn");
			expect(isBattle).toBe(true);
		}).toPass({ timeout: 120_000, intervals: [2000] });
		console.log("[Test] Battle phase started!");

		async function getInstructions(): Promise<string> {
			return (await page.locator("#instructions").textContent()) || "";
		}

		async function getStatus(): Promise<string> {
			return (await page.locator("#status").textContent()) || "";
		}

		function isGameOver(text: string): boolean {
			return text.includes("Victory") || text.includes("Defeat");
		}

		function isOurTurn(text: string): boolean {
			return text.includes("click enemy waters");
		}

		let attackCount = 0;
		const canvas = page.locator("#enemy-board");
		const box = await canvas.boundingBox();
		expect(box).not.toBeNull();

		for (let cellIdx = 0; cellIdx < 100; cellIdx++) {
			if (isGameOver(await getInstructions())) break;

			await expect(canvas).toHaveAttribute("data-can-attack", "true", { timeout: 120_000 });

			if (isGameOver(await getInstructions())) break;

			const cellX = cellIdx % 10;
			const cellY = Math.floor(cellIdx / 10);
			await page.mouse.click(box!.x + gridToScreen(cellX, cellY).x, box!.y + gridToScreen(cellX, cellY).y);
			attackCount++;
			if (attackCount % 10 === 1) {
				console.log(`[Test] Attacked cell (${cellX},${cellY}), total attacks: ${attackCount}`);
			}

			await expect(canvas).toHaveAttribute("data-can-attack", "false", { timeout: 10_000 });
		}

		console.log(`[Test] Total attacks made: ${attackCount}`);

		let finalInstructions = "";
		let finalStatus = "";
		await expect(async () => {
			finalInstructions = (await page.locator("#instructions").textContent()) || "";
			finalStatus = (await page.locator("#status").textContent()) || "";
			const ended = finalInstructions.includes("Victory") || finalInstructions.includes("Defeat");
			expect(ended).toBe(true);
		}).toPass({ timeout: 1_800_000, intervals: [1000] });

		console.log(`[Test] Final instructions: ${finalInstructions}`);
		console.log(`[Test] Final status: ${finalStatus}`);

		const isVictory = finalInstructions.includes("Victory");
		const isDefeat = finalInstructions.includes("Defeat");
		expect(isVictory || isDefeat).toBe(true);
		expect(attackCount).toBeGreaterThan(0);
	});
	}
});
