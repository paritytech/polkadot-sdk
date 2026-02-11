import { WebSocket } from "ws";
import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import { Binary, AccountId } from "polkadot-api";
import { startProxy } from "../src/chain/smoldot-proxy.ts";
import type { Chain } from "smoldot";

(globalThis as any).WebSocket = WebSocket;

function createWsChain(wsUrl: string): Promise<Chain> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const responseQueue: string[] = [];
    let responseWaiter: ((response: string) => void) | null = null;
    let closed = false;

    ws.on("open", () => {
      const chain: Chain = {
        sendJsonRpc(rpc: string) { if (ws.readyState === WebSocket.OPEN) ws.send(rpc); },
        nextJsonRpcResponse(): Promise<string> {
          if (responseQueue.length > 0) return Promise.resolve(responseQueue.shift()!);
          if (closed) return Promise.reject(new Error("closed"));
          return new Promise((res) => { responseWaiter = res; });
        },
        remove() { closed = true; ws.close(); },
      };
      resolve(chain);
    });

    ws.on("message", (event: any) => {
      const data = typeof event === "string" ? event : event.toString();
      if (responseWaiter) { const w = responseWaiter; responseWaiter = null; w(data); }
      else responseQueue.push(data);
    });

    ws.on("error", (e: any) => { if (!closed) reject(e); });
    ws.on("close", () => { closed = true; });
  });
}

const COLLATOR_WS = "ws://127.0.0.1:35729";
function rpcCall(method: string, params: any[] = []): Promise<any> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(COLLATOR_WS);
    ws.on("open", () => ws.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params })));
    ws.on("message", (msg: any) => { const d = JSON.parse(msg.toString()); ws.close(); d.error ? reject(d.error) : resolve(d.result); });
    ws.on("error", (e: any) => reject(e));
  });
}

async function main() {
  const proxy = await startProxy(9997);
  console.log("Proxy started, waiting 20s for sync...");
  await new Promise(r => setTimeout(r, 20000));

  console.log("Creating PAPI client via proxy...");
  const smProvider = getSmProvider(() => createWsChain(`ws://127.0.0.1:${proxy.port}`));
  const client = createClient(smProvider);

  const { firstValueFrom } = await import("rxjs");
  console.log("Waiting for bestBlock...");
  await firstValueFrom(client.bestBlocks$);
  console.log("Got bestBlock");

  const entropy = mnemonicToEntropy(DEV_PHRASE);
  const derive = sr25519CreateDerive(entropyToMiniSecret(entropy));
  const aliceKeyPair = derive("//Alice");
  const aliceSigner = getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", (input) => aliceKeyPair.sign(input));
  const aliceAddress = AccountId().dec(aliceKeyPair.publicKey);

  // Get nonce from PAPI (smoldot best block)
  const api = client.getUnsafeApi() as any;
  const acct = await api.query.System.Account.getValue(aliceAddress, { at: "best" });
  const papiNonce = acct?.nonce ?? 0;

  // Get nonce from direct RPC
  const directNonce = await rpcCall("system_accountNextIndex", [aliceAddress]);
  
  console.log(`PAPI nonce: ${papiNonce}, Direct RPC nonce: ${directNonce}, diff: ${directNonce - papiNonce}`);

  // Submit a remark tx using the DIRECT nonce (correct one)
  const remarkTx = api.tx.System.remark({ remark: Binary.fromText("test" + Date.now()) });
  const signedTx = await remarkTx.sign(aliceSigner, { mortality: { mortal: false }, nonce: directNonce });
  const hexTx = "0x" + Array.from(signedTx as Uint8Array).map((b: number) => b.toString(16).padStart(2, '0')).join('');

  console.log(`Submitting remark tx with nonce=${directNonce}...`);
  const request = (client as any)._request;
  await request("author_submitExtrinsic", [hexTx]);
  console.log("Submitted!");

  // Wait and check if nonce incremented
  for (let i = 0; i < 20; i++) {
    await new Promise(r => setTimeout(r, 2000));
    const currentNonce = await rpcCall("system_accountNextIndex", [aliceAddress]);
    console.log(`[${i * 2}s] Direct nonce: ${currentNonce}`);
    if (currentNonce > directNonce) {
      console.log("TX INCLUDED! Test PASSED");
      client.destroy();
      proxy.stop();
      process.exit(0);
    }
  }

  console.log("TX NOT INCLUDED after 40s. Test FAILED");
  client.destroy();
  proxy.stop();
  process.exit(1);
}

main();
