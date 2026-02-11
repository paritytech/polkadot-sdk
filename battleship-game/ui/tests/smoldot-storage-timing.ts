import { start } from "smoldot";
import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { relayChainSpec, parachainSpec } from "../src/chain/chainSpecs.ts";

async function main() {
  console.log("=== Smoldot Storage Query Timing Test ===\n");

  const smoldot = start({
    maxLogLevel: 4,
    logCallback: (level, _target, message) => {
      if (level <= 2) {
        console.log(`  [smoldot:${level === 1 ? "ERROR" : "WARN"}] ${message.slice(0, 200)}`);
      }
      if (message.includes("finali") || message.includes("Finali")) {
        console.log(`  [smoldot:finality] ${message.slice(0, 300)}`);
      }
    },
  });

  const relayChain = await smoldot.addChain({ chainSpec: relayChainSpec });
  const smProvider = getSmProvider(() =>
    smoldot.addChain({
      chainSpec: parachainSpec,
      potentialRelayChains: [relayChain],
    })
  );

  const client = createClient(smProvider);

  console.log("Waiting for first best block...");
  let firstBlock = false;
  await new Promise<void>((resolve) => {
    const sub = (client.bestBlocks$ as any).subscribe({
      next: () => {
        if (!firstBlock) {
          firstBlock = true;
          sub.unsubscribe();
          resolve();
        }
      },
    });
  });
  console.log("Got first best block.\n");

  await new Promise((r) => setTimeout(r, 3000));

  console.log("--- Test 1: system_name RPC ---");
  const t1 = Date.now();
  try {
    const result = await (client as any)._request("system_name", []);
    console.log(`  ${Date.now() - t1}ms, result=${result}`);
  } catch (e: any) {
    console.log(`  ${Date.now() - t1}ms, ERROR: ${e.message?.slice(0, 100)}`);
  }

  console.log("\n--- Test 2: bestBlocks$ values ---");
  for (let i = 0; i < 3; i++) {
    const t0 = Date.now();
    const blocks: any = await new Promise((resolve) => {
      let subscription: any;
      subscription = (client.bestBlocks$ as any).subscribe({
        next: (b: any) => { if (subscription) subscription.unsubscribe(); resolve(b); },
      });
    });
    console.log(`  #${i + 1}: ${Date.now() - t0}ms, best=${blocks[0]?.hash?.slice(0, 18)}... number=${blocks[0]?.number}`);
  }

  console.log("\n--- Test 3: finalizedBlock$ value ---");
  const t3 = Date.now();
  const finBlock: any = await new Promise((resolve) => {
    let subscription: any;
    subscription = (client.finalizedBlock$ as any).subscribe({
      next: (b: any) => { if (subscription) subscription.unsubscribe(); resolve(b); },
    });
  });
  console.log(`  ${Date.now() - t3}ms, hash=${finBlock?.hash?.slice(0, 18)}... number=${finBlock?.number}`);

  console.log("\n--- Test 4: Monitor finalizedBlock$ changes for 20s ---");
  let finalizedCount = 0;
  let lastFinalizedNumber = 0;
  const finSub = (client.finalizedBlock$ as any).subscribe({
    next: (block: { hash: string; number: number }) => {
      finalizedCount++;
      if (block?.number !== lastFinalizedNumber) {
        console.log(`  Finalized: number=${block?.number} hash=${block?.hash?.slice(0, 18)}...`);
        lastFinalizedNumber = block?.number;
      }
    },
  });
  await new Promise((r) => setTimeout(r, 20000));
  finSub.unsubscribe();
  console.log(`  Total finalized notifications in 20s: ${finalizedCount}`);

  console.log("\n--- Test 5: Monitor bestBlocks$ for 10s ---");
  let bestCount = 0;
  const bestSub = (client.bestBlocks$ as any).subscribe({
    next: (blocks: any[]) => {
      bestCount++;
      if (bestCount <= 5 || bestCount % 10 === 0) {
        console.log(`  Best #${bestCount}: number=${blocks[0]?.number} hash=${blocks[0]?.hash?.slice(0, 18)}...`);
      }
    },
  });
  await new Promise((r) => setTimeout(r, 10000));
  bestSub.unsubscribe();
  console.log(`  Total best block notifications in 10s: ${bestCount}`);

  console.log("\nDone.");
  client.destroy();
  smoldot.terminate();
  process.exit(0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
