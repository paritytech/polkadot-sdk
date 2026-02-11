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
  const paraChain = await smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] });
  
  const client = createClient(getSmProvider(paraChain));
  
  // Try bestBlocks$ instead of getFinalizedBlock()
  console.log("Subscribing to bestBlocks$...");
  const sub1 = client.bestBlocks$.subscribe({
    next: (blocks: any) => {
      console.log(`bestBlocks: ${blocks?.length || 0} blocks, first: #${blocks?.[0]?.number}`);
    },
    error: (err: any) => console.error("bestBlocks error:", err.message?.slice(0, 200))
  });

  // Also try _request for raw JSON-RPC
  console.log("Trying _request for chain_getBlockHash...");
  try {
    const result = await (client as any)._request("chain_getBlockHash", [0]);
    console.log("_request result:", result);
  } catch(e: any) {
    console.log("_request not available or failed:", e.message?.slice(0, 100));
  }

  // Wait and see what happens
  await new Promise(r => setTimeout(r, 20000));
  sub1.unsubscribe();
  
  console.log("\nNow trying getUnsafeApi query...");
  try {
    const api = client.getUnsafeApi();
    const nextId = await api.query.Battleship.NextGameId.getValue({ at: "best" });
    console.log("NextGameId:", nextId);
  } catch(e: any) {
    console.log("Query failed:", e.message?.slice(0, 200));
  }

  client.destroy();
  smoldot.terminate();
  process.exit(0);
}

main();
