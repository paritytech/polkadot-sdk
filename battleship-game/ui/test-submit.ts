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

async function main() {
  const relaySpec = fs.readFileSync("public/chain-specs/relay.json", "utf-8");
  const paraSpec = fs.readFileSync("public/chain-specs/parachain.json", "utf-8");
  const smoldot = start({ maxLogLevel: 3 });
  const relayChain = await smoldot.addChain({ chainSpec: relaySpec });

  // Create a separate chain for raw JSON-RPC
  const rawChain = await smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] });
  
  const client = createClient(getSmProvider(() => 
    smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] })
  ));
  
  const block = await client.getFinalizedBlock();
  console.log("Synced:", block.number);
  
  const api = client.getUnsafeApi();
  
  // Check nonce before
  rawChain.sendJsonRpc(JSON.stringify({
    jsonrpc: "2.0", id: 1, method: "system_accountNextIndex",
    params: ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"]
  }));
  const nonceResp = JSON.parse(await rawChain.nextJsonRpcResponse());
  console.log("Nonce before:", nonceResp.result);
  
  // Get the encoded extrinsic using decodedCall approach
  const tx = api.tx.System.remark({ remark: Binary.fromText(`raw-submit-${Date.now()}`) });
  
  // Use getEncodedData to get the raw tx bytes, then sign manually
  // Actually, let's try tx.sign and then submit the hex via raw JSON-RPC
  console.log("Signing...");
  const signedTx = await tx.sign(signer, { mortality: { mortal: false } });
  console.log("Signed tx hex length:", signedTx.length);
  
  // Submit via raw JSON-RPC author_submitExtrinsic
  const hexTx = "0x" + Buffer.from(signedTx).toString("hex");
  console.log("Submitting via raw JSON-RPC...");
  rawChain.sendJsonRpc(JSON.stringify({
    jsonrpc: "2.0", id: 2, method: "author_submitExtrinsic",
    params: [hexTx]
  }));
  const submitResp = JSON.parse(await rawChain.nextJsonRpcResponse());
  console.log("Submit response:", JSON.stringify(submitResp));
  
  // Wait for inclusion
  console.log("Waiting 15s for inclusion...");
  await new Promise(r => setTimeout(r, 15000));
  
  // Check nonce after
  rawChain.sendJsonRpc(JSON.stringify({
    jsonrpc: "2.0", id: 3, method: "system_accountNextIndex",
    params: ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"]
  }));
  const nonceResp2 = JSON.parse(await rawChain.nextJsonRpcResponse());
  console.log("Nonce after:", nonceResp2.result);
  
  if (nonceResp2.result > nonceResp.result) {
    console.log("TX INCLUDED! Nonce incremented.");
  } else {
    console.log("TX NOT INCLUDED (nonce unchanged)");
  }
  
  client.destroy();
  rawChain.remove();
  smoldot.terminate();
}

main().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
