import { start } from "smoldot";
import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import { Binary, AccountId } from "polkadot-api";
import { relayChainSpec, parachainSpec } from "../src/chain/chainSpecs";
import WebSocket from "ws";

(globalThis as any).WebSocket = WebSocket;

const COLLATOR_WS = "ws://127.0.0.1:35729";

function rpcCall(method: string, params: any[] = []): Promise<any> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(COLLATOR_WS);
    ws.onopen = () => ws.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }));
    ws.onmessage = (msg: any) => { const d = JSON.parse(msg.data); ws.close(); d.error ? reject(d.error) : resolve(d.result); };
    ws.onerror = (e: any) => reject(e);
  });
}

async function main() {
  console.log("Starting smoldot...");
  const smoldot = start({ maxLogLevel: 3 });
  const relay = await smoldot.addChain({ chainSpec: relayChainSpec });
  console.log("Relay added");
  console.log("Setting up parachain provider...");
  const client = createClient(getSmProvider(() => smoldot.addChain({ chainSpec: parachainSpec, potentialRelayChains: [relay] })));
  console.log("Client created, waiting for bestBlocks (max 60s)...");
  const waitStart = Date.now();
  const bestBlock = await Promise.race([
    (async () => { const { firstValueFrom } = await import("rxjs"); return firstValueFrom(client.bestBlocks$); })(),
    new Promise<never>((_, rej) => setTimeout(() => rej(new Error("bestBlocks TIMEOUT 60s")), 60000)),
  ]);
  console.log(`Got bestBlock in ${Date.now() - waitStart}ms:`, JSON.stringify(bestBlock).slice(0, 200));

  const entropy = mnemonicToEntropy(DEV_PHRASE);
  const derive = sr25519CreateDerive(entropyToMiniSecret(entropy));
  const aliceKeyPair = derive("//Alice");
  const aliceSigner = getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", (input) => aliceKeyPair.sign(input));
  const aliceAddress = AccountId().dec(aliceKeyPair.publicKey);

  const rpcNonce0 = await rpcCall("system_accountNextIndex", [aliceAddress]);
  console.log(`Initial RPC nonce: ${rpcNonce0}`);

  // Test chainHead events from smoldot
  console.log("Testing chainHead subscription...");
  const chainHeadResult = await Promise.race([
    (client as any)._request("chain_getBlockHash", []),
    new Promise<string>((_, rej) => setTimeout(() => rej(new Error("chain_getBlockHash TIMEOUT 10s")), 10000)),
  ]).catch((e: any) => `ERROR: ${e.message}`);
  console.log(`chain_getBlockHash: ${chainHeadResult}`);

  // Check if bestBlocks$ emits
  console.log("Checking bestBlocks$...");
  const bestBlockP = Promise.race([
    client.getFinalizedBlock(),
    new Promise<any>((_, rej) => setTimeout(() => rej(new Error("getFinalizedBlock TIMEOUT 15s")), 15000)),
  ]).catch((e: any) => `ERROR: ${e.message}`);
  const bestResult = await bestBlockP;
  console.log(`getFinalizedBlock: ${JSON.stringify(bestResult)}`);

  // Now try building a tx
  console.log("Building tx...");
  let hexTx: string;
  try {
    hexTx = await Promise.race([
      (async () => {
        const api = client.getUnsafeApi();
        const remarkTx = api.tx.System.remark({ remark: Binary.fromText("t" + Date.now()) });
        const signedTx = await remarkTx.sign(aliceSigner, { mortality: { mortal: false }, nonce: rpcNonce0 });
        return "0x" + Array.from(signedTx as Uint8Array).map(b => b.toString(16).padStart(2, '0')).join('');
      })(),
      new Promise<never>((_, rej) => setTimeout(() => rej(new Error("TX BUILD TIMEOUT 20s")), 20000)),
    ]);
  } catch (e) {
    console.error("Failed to build tx:", e);
    await smoldot.terminate();
    process.exit(1);
  }
  console.log(`Tx built (${hexTx.length} chars)`);

  const t0 = Date.now();
  console.log(`[0ms] Submitting via smoldot...`);
  await (client as any)._request("author_submitExtrinsic", [hexTx]);
  console.log(`[${Date.now() - t0}ms] Submitted`);

  const expected = rpcNonce0 + 1;
  let rpcT = 0, smoldotT = 0;

  for (let i = 0; i < 60; i++) {
    await new Promise(r => setTimeout(r, 500));
    const elapsed = Date.now() - t0;

    if (!rpcT) {
      try { const n = await rpcCall("system_accountNextIndex", [aliceAddress]); if (n >= expected) { rpcT = elapsed; console.log(`[${elapsed}ms] ✓ RPC nonce=${n}`); } } catch {}
    }
    if (!smoldotT) {
      try { const n = await (client as any)._request("system_accountNextIndex", [aliceAddress]) as number; if (n >= expected) { smoldotT = elapsed; console.log(`[${elapsed}ms] ✓ Smoldot nonce=${n}`); } } catch {}
    }
    if (rpcT && smoldotT) break;
    if (i % 4 === 3) console.log(`  [${elapsed}ms] RPC=${rpcT ? 'done' : '-'} Smoldot=${smoldotT ? 'done' : '-'}`);
  }

  console.log(`\n=== RESULTS ===`);
  console.log(`TX gossip (smoldot→collator): ${rpcT || 'NOT DETECTED'}ms`);
  console.log(`Block detection (smoldot sees inclusion): ${smoldotT || 'NOT DETECTED'}ms`);
  if (rpcT && smoldotT) console.log(`Block tracking gap: ${smoldotT - rpcT}ms`);
  if (rpcT > 5000) console.log(`→ TX GOSSIP IS THE BOTTLENECK`);
  else if (rpcT && smoldotT && (smoldotT - rpcT) > 3000) console.log(`→ BLOCK DETECTION IS THE BOTTLENECK`);
  else if (rpcT && smoldotT) console.log(`→ Both fast, total: ${smoldotT}ms`);

  await smoldot.terminate();
  process.exit(0);
}

main().catch(e => { console.error(e); process.exit(1); });
