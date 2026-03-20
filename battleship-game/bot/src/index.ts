import { getClient, getStatementChain, createStatementChain, disconnectClient } from "./client.js";
import { BattleshipClient } from "./battleship.js";
import { BattleshipBot } from "./bot.js";
import { createRandomAccount } from "./accounts.js";
import { StatementStoreClient } from "./statementStore.js";

async function main() {
	const instanceCount = parseInt(process.argv[2] || "1", 10);
	if (isNaN(instanceCount) || instanceCount < 1) {
		console.error("Usage: battleship-bot [count]");
		process.exit(1);
	}

	console.log("=".repeat(60));
	console.log(`Battleship Bot Starting (${instanceCount} instance${instanceCount > 1 ? "s" : ""})`);
	console.log("=".repeat(60));

	const client = await getClient();
	const battleshipClient = await BattleshipClient.create(client);

	const bots: BattleshipBot[] = [];

	for (let i = 0; i < instanceCount; i++) {
		const account = createRandomAccount();
		const label = `Bot-${i + 1}`;
		console.log(`[${label}] Address: ${account.address}`);

		console.log(`[${label}] Requesting funds...`);
		await battleshipClient.requestFunds(account.address);

		console.log(`[${label}] Waiting for funds...`);
		const funded = await battleshipClient.waitForFunds(account.address, 60_000);
		if (!funded) {
			console.error(`[${label}] Faucet tx was not finalized in time`);
			process.exit(1);
		}
		console.log(`[${label}] Account funded`);

		let statementStore: StatementStoreClient | undefined;
		try {
			// First instance uses getStatementChain, others create new chains
			const stmtChain = i === 0
				? await getStatementChain()
				: await createStatementChain();
			if (stmtChain) {
				statementStore = new StatementStoreClient(stmtChain);
				console.log(`[${label}] Statement store initialized`);
			}
		} catch (e) {
			console.warn(`[${label}] Failed to initialize statement store:`, e);
		}

		const bot = new BattleshipBot(battleshipClient, account, statementStore);
		bots.push(bot);
	}

	console.log("=".repeat(60));
	console.log(`Starting ${bots.length} bot instance(s)...`);
	console.log("=".repeat(60));

	// Run all bots concurrently
	await Promise.all(bots.map(bot => bot.run()));
}

process.on("SIGINT", () => {
	console.log("\n[Bot] Shutting down...");
	disconnectClient();
	process.exit(0);
});

process.on("SIGTERM", () => {
	console.log("\n[Bot] Shutting down...");
	disconnectClient();
	process.exit(0);
});

main();
