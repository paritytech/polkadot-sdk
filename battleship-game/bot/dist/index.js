import { getClient, disconnectClient } from "./client.js";
import { BattleshipClient } from "./battleship.js";
import { BattleshipBot } from "./bot.js";
import { createRandomAccount } from "./accounts.js";
async function main() {
    const botAccount = createRandomAccount();
    console.log("=".repeat(60));
    console.log("Battleship Bot Starting");
    console.log("=".repeat(60));
    console.log(`Bot Address: ${botAccount.address}`);
    console.log(`Using: Smoldot light client`);
    console.log("=".repeat(60));
    try {
        const client = await getClient();
        const battleshipClient = await BattleshipClient.create(client);
        // Request funds from faucet
        console.log("[Bot] Requesting funds from faucet...");
        await battleshipClient.requestFunds(botAccount.address);
        // Wait for the faucet tx to be included
        await new Promise(r => setTimeout(r, 6000));
        console.log("[Bot] Faucet request submitted, proceeding...");
        const bot = new BattleshipBot(battleshipClient, botAccount);
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
