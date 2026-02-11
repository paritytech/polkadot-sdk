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
  const client = createClient(getSmProvider(() => 
    smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] })
  ));
  
  const block = await client.getFinalizedBlock();
  console.log("Synced:", block.number);
  
  const api = client.getUnsafeApi();
  const tx = api.tx.System.remark({ remark: Binary.fromText("immortal-test") });
  
  try {
    const result = await new Promise((resolve, reject) => {
      const sub = tx.signSubmitAndWatch(signer, { mortality: { mortal: false } }).subscribe({
        next: (e: any) => {
          console.log("event:", e.type, e.found !== undefined ? `found=${e.found}` : '', e.block ? `block=${e.block.number}` : '');
          if (e.type === "txBestBlocksState" && e.found === true) { sub.unsubscribe(); resolve("OK"); }
          if (e.type === "invalid" || e.type === "dropped") { sub.unsubscribe(); reject(new Error(`TX ${e.type}: ${JSON.stringify(e)}`)); }
        },
        error: (err: any) => { sub.unsubscribe(); reject(err); }
      });
      setTimeout(() => { sub.unsubscribe(); reject(new Error("timeout")); }, 120000);
    });
    console.log("TX SUCCESS!");
  } catch(e: any) {
    console.error("TX FAILED:", e.message?.slice(0, 300));
  }
  
  client.destroy();
  smoldot.terminate();
}

main().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
