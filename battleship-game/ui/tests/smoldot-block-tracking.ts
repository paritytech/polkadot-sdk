import { start } from "smoldot";
import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { WebSocket } from "ws";
import { relayChainSpec, parachainSpec } from "../src/chain/chainSpecs.ts";

const WS_URL = process.env.WS_URL || "ws://localhost:9944";
const TEST_DURATION_MS = 120_000;

async function getRpcBlockNumber(wsUrl: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    ws.on("open", () => {
      ws.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "chain_getHeader", params: [] }));
    });
    ws.on("message", (data: Buffer) => {
      const msg = JSON.parse(data.toString());
      ws.close();
      resolve(parseInt(msg.result.number, 16));
    });
    ws.on("error", reject);
    setTimeout(() => reject(new Error("RPC timeout")), 10000);
  });
}

async function main() {
  console.log("=== Smoldot Block Tracking Test ===\n");
  console.log(`Reference RPC: ${WS_URL}`);

  const rpcBlock = await getRpcBlockNumber(WS_URL);
  console.log(`Current chain head via RPC: #${rpcBlock}\n`);

  console.log("Starting smoldot...");
  const smoldot = start({
    maxLogLevel: 4,
    logCallback: (level, target, message) => {
      if (level <= 2) {
        const names = ["", "ERROR", "WARN"];
        console.log(`  [smoldot:${names[level]}] [${target}] ${message.slice(0, 200)}`);
      }
      if (level >= 3) {
        if (
          message.includes("sync mode") ||
          message.includes("routing") ||
          message.includes("SUBSTRATE") ||
          message.includes("PARACHAIN") ||
          message.includes("initialization") ||
          message.includes("warp sync finished") ||
          message.includes("Fetching") ||
          message.includes("Got para") ||
          message.includes("Fetched para") ||
          message.includes("falling back") ||
          message.includes("verify-error") ||
          (message.includes("verify-success") && !message.includes("warp-sync"))
        ) {
          const names = ["", "", "", "INFO", "DEBUG"];
          console.log(`  [smoldot:${names[level] || "TRACE"}] [${target}] ${message.slice(0, 300)}`);
        }
      }
    },
  });

  console.log("Adding relay chain...");
  const relayChain = await smoldot.addChain({ chainSpec: relayChainSpec });

  console.log("Adding parachain...");
  const smProvider = getSmProvider(() =>
    smoldot.addChain({
      chainSpec: parachainSpec,
      potentialRelayChains: [relayChain],
    })
  );

  console.log("Creating PAPI client...");
  const client = createClient(smProvider);

  const blockTimes: { blockHash: string; timestamp: number }[] = [];
  const startTime = Date.now();
  let firstBlockTime: number | null = null;

  console.log(`\nSubscribing to bestBlocks$ for ${TEST_DURATION_MS / 1000}s...\n`);

  const sub = (client.bestBlocks$ as any).subscribe({
    next: (blocks: { hash: string; number: number }[]) => {
      const now = Date.now();
      const elapsed = now - startTime;
      const block = blocks[0];

      if (!firstBlockTime) {
        firstBlockTime = now;
        console.log(`  First best block after ${elapsed}ms: hash=${block?.hash?.slice(0, 18)}...`);
      }

      const prevTime = blockTimes.length > 0 ? blockTimes[blockTimes.length - 1].timestamp : now;
      const gap = now - prevTime;
      blockTimes.push({ blockHash: block?.hash || "?", timestamp: now });

      if (blockTimes.length <= 20 || blockTimes.length % 10 === 0) {
        console.log(
          `  Block #${blockTimes.length} at +${elapsed}ms (gap=${gap}ms) hash=${block?.hash?.slice(0, 18)}...`
        );
      }
    },
    error: (err: unknown) => console.error("bestBlocks$ error:", err),
  });

  await new Promise((r) => setTimeout(r, TEST_DURATION_MS));
  sub.unsubscribe();

  console.log("\n=== Results ===\n");
  console.log(`Total best block notifications: ${blockTimes.length}`);

  if (blockTimes.length < 2) {
    console.log("FAIL: Too few blocks received to analyze timing.");
    client.destroy();
    smoldot.terminate();
    process.exit(1);
  }

  const gaps = blockTimes.slice(1).map((b, i) => b.timestamp - blockTimes[i].timestamp);
  gaps.sort((a, b) => a - b);
  const avgGap = gaps.reduce((a, b) => a + b, 0) / gaps.length;
  const medianGap = gaps[Math.floor(gaps.length / 2)];
  const minGap = gaps[0];
  const maxGap = gaps[gaps.length - 1];
  const p90Gap = gaps[Math.floor(gaps.length * 0.9)];

  console.log(`Time to first block: ${firstBlockTime! - startTime}ms`);
  console.log(`Block gap (avg): ${avgGap.toFixed(0)}ms`);
  console.log(`Block gap (median): ${medianGap}ms`);
  console.log(`Block gap (min): ${minGap}ms`);
  console.log(`Block gap (max): ${maxGap}ms`);
  console.log(`Block gap (p90): ${p90Gap}ms`);

  const rpcBlockAfter = await getRpcBlockNumber(WS_URL);
  console.log(`\nChain head via RPC after test: #${rpcBlockAfter}`);
  console.log(`Blocks produced during test: ${rpcBlockAfter - rpcBlock}`);
  console.log(`Blocks observed by smoldot: ${blockTimes.length}`);

  if (medianGap > 10000) {
    console.log(
      "\nFAIL: Median gap > 10s — smoldot is likely using relay-chain finalization, not direct p2p."
    );
    client.destroy();
    smoldot.terminate();
    process.exit(1);
  }

  if (medianGap <= 2000) {
    console.log("\nPASS: Median gap <= 2s — smoldot is tracking blocks via direct p2p!");
  } else {
    console.log(
      `\nWARN: Median gap ${medianGap}ms — faster than finalization but not real-time p2p.`
    );
  }

  client.destroy();
  smoldot.terminate();
  process.exit(0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
