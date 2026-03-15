import { getClient, getStatementChain, disconnectClient } from "./client.js";
import { BattleshipClient } from "./battleship.js";
import { BattleshipBot } from "./bot.js";
import { createRandomAccount, createAccountFromMnemonic } from "./accounts.js";
import { getStatementStore } from "./statementStore.js";
async function main() {
    const seed = process.env.BOT_SEED;
    const botAccount = seed ? createAccountFromMnemonic(seed) : createRandomAccount();
    console.log("=".repeat(60));
    console.log("Battleship Bot Starting");
    console.log("=".repeat(60));
    console.log(`Bot Address: ${botAccount.address}`);
    console.log(`Account: ${seed ? "from BOT_SEED" : "random (new)"}`);
    console.log(`Using: Smoldot light client`);
    console.log("=".repeat(60));
    try {
        const client = await getClient();
        const battleshipClient = await BattleshipClient.create(client);
        // Request funds from faucet (also sets statement store allowance)
        console.log("[Bot] Requesting funds from faucet...");
        await battleshipClient.requestFunds(botAccount.address);
        // Wait for faucet tx to be finalized — the statement store reads
        // allowances from finalized state, so we must confirm it's there.
        console.log("[Bot] Waiting for faucet tx to be finalized...");
        const funded = await battleshipClient.waitForFunds(botAccount.address, 60_000);
        if (!funded) {
            console.error("[Bot] Faucet tx was not finalized in time");
            process.exit(1);
        }
        console.log("[Bot] Account funded and finalized");
        // Initialize statement store for game announcements + ping/pong
        let statementStore = undefined;
        try {
            const stmtChain = await getStatementChain();
            if (stmtChain) {
                statementStore = getStatementStore(stmtChain);
                console.log("[Bot] Statement store initialized");
            }
        }
        catch (e) {
            console.warn("[Bot] Failed to initialize statement store:", e);
        }
        const bot = new BattleshipBot(battleshipClient, botAccount, statementStore);
        await bot.run();
    }
    catch (e) {
        console.error("Fatal error:", e);
        disconnectClient();
        process.exit(1);
    }
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
