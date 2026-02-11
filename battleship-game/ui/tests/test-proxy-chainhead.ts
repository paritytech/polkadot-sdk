import { WebSocket } from "ws";
import { startProxy } from "../src/chain/smoldot-proxy.ts";

async function test() {
  (globalThis as any).WebSocket = WebSocket;

  console.log("Starting proxy...");
  const proxy = await startProxy(9998);
  console.log("Waiting 10s for sync...");
  await new Promise(r => setTimeout(r, 10000));

  const ws = new WebSocket(`ws://127.0.0.1:${proxy.port}`);

  ws.on("open", () => {
    console.log("Connected to proxy");
    const followReq = {
      jsonrpc: "2.0",
      id: "follow-1",
      method: "chainHead_v1_follow",
      params: [true],
    };
    console.log("Sending:", JSON.stringify(followReq));
    ws.send(JSON.stringify(followReq));
  });

  let msgCount = 0;
  ws.on("message", (data: any) => {
    msgCount++;
    const msg = JSON.parse(data.toString());
    if (msg.params?.result?.event) {
      console.log(`[${msgCount}] Notification: event=${msg.params.result.event}`);
      if (msg.params.result.event === "initialized") {
        console.log(`  Finalized: ${JSON.stringify(msg.params.result.finalizedBlockHashes)}`);
      }
      if (msg.params.result.event === "bestBlockChanged") {
        console.log(`  Best block: ${msg.params.result.bestBlockHash}`);
      }
      if (msg.params.result.event === "newBlock") {
        console.log(`  New block: ${msg.params.result.blockHash}, parent: ${msg.params.result.parentBlockHash}`);
      }
    } else {
      console.log(`[${msgCount}] Response:`, JSON.stringify(msg).slice(0, 300));
    }

    if (msgCount >= 20) {
      console.log("\nGot 20 messages, test PASSED");
      ws.close();
      proxy.stop();
      process.exit(0);
    }
  });

  ws.on("error", (e: any) => console.error("WS error:", e));

  setTimeout(() => {
    console.log(`\nTimeout: received ${msgCount} messages total`);
    if (msgCount === 0) {
      console.log("FAILED: No messages from proxy");
    } else if (msgCount === 1) {
      console.log("FAILED: Only got subscription response, no notifications");
    } else {
      console.log("PARTIAL: Got some messages but not enough");
    }
    ws.close();
    proxy.stop();
    process.exit(1);
  }, 30000);
}

test();
