import { getClient, disconnectClient } from "./client.js";
import { BattleshipClient } from "./battleship.js";
import { BattleshipBot } from "./bot.js";
import { botAccount } from "./accounts.js";

async function main() {
  console.log("=".repeat(60));
  console.log("Battleship Bot Starting");
  console.log("=".repeat(60));
  console.log(`Bot Address: ${botAccount.address}`);
  console.log(`Using: Smoldot light client`);
  console.log("=".repeat(60));

  try {
    // Connect to chain via smoldot
    const client = await getClient();
    const battleshipClient = await BattleshipClient.create(client);

    // Start bot
    const bot = new BattleshipBot(battleshipClient, botAccount);
    await bot.run();
  } catch (e) {
    console.error("Fatal error:", e);
    disconnectClient();
    process.exit(1);
  }
}

// Handle graceful shutdown
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
