import { createClient, Binary } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { start } from "smoldot";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import * as fs from "fs";

const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);
const alice = derive("//Alice");
const signer = getPolkadotSigner(alice.publicKey, "Sr25519", alice.sign);

const RPC_URL = "http://127.0.0.1:37039";

async function submitViaHttp(hexTx: string): Promise<any> {
  const resp = await fetch(RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0", id: 1,
      method: "author_submitExtrinsic",
      params: [hexTx]
    })
  });
  return resp.json();
}

async function getNonce(): Promise<number> {
  const resp = await fetch(RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0", id: 1,
      method: "system_accountNextIndex",
      params: ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"]
    })
  });
  const data = await resp.json();
  return data.result;
}

async function main() {
  const relaySpec = fs.readFileSync("public/chain-specs/relay.json", "utf-8");
  const paraSpec = fs.readFileSync("public/chain-specs/parachain.json", "utf-8");
  const smoldot = start({ maxLogLevel: 3 });
  const relayChain = await smoldot.addChain({ chainSpec: relaySpec });
  const client = createClient(getSmProvider(() => 
    smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] })
  ));
  
  const block = await client.getFinalizedBlock();
  console.log("Synced:", block.number);
  
  const nonceBefore = await getNonce();
  console.log("Nonce before:", nonceBefore);
  
  // Sign tx using PAPI 2.0 + smoldot (correct signing)
  const api = client.getUnsafeApi();
  const tx = api.tx.System.remark({ remark: Binary.fromText(`http-submit-${Date.now()}`) });
  const signedTx = await tx.sign(signer, { mortality: { mortal: false } });
  const hexTx = "0x" + Buffer.from(signedTx).toString("hex");
  
  // Submit via HTTP POST (not WebSocket!)
  console.log("Submitting via HTTP POST...");
  const result = await submitViaHttp(hexTx);
  console.log("HTTP submit result:", JSON.stringify(result));
  
  // Wait for inclusion
  console.log("Waiting 12s...");
  await new Promise(r => setTimeout(r, 12000));
  
  const nonceAfter = await getNonce();
  console.log("Nonce after:", nonceAfter);
  
  if (nonceAfter > nonceBefore) {
    console.log("SUCCESS! Transaction included via HTTP submit.");
  } else {
    console.log("FAILED - tx not included");
  }
  
  client.destroy();
  smoldot.terminate();
}

main().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
