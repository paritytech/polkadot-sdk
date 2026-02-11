import { createClient, Binary } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";

const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);
const alice = derive("//Alice");
const signer = getPolkadotSigner(alice.publicKey, "Sr25519", alice.sign);

async function main() {
  const client = createClient(getWsProvider("ws://localhost:37039"));
  const block = await client.getFinalizedBlock();
  console.log("WS synced:", block.number);
  const api = client.getUnsafeApi();
  const tx = api.tx.System.remark({ remark: Binary.fromText("ws-papi2-retest") });
  try {
    const result = await new Promise((resolve, reject) => {
      const sub = tx.signSubmitAndWatch(signer).subscribe({
        next: (e: any) => {
          console.log("event:", e.type, e.found !== undefined ? `found=${e.found}` : '', e.block ? `block=${e.block.number}` : '');
          if (e.type === "txBestBlocksState" && e.found === true) { sub.unsubscribe(); resolve("OK"); }
          if (e.type === "invalid" || e.type === "dropped") { sub.unsubscribe(); reject(new Error(`TX ${e.type}: ${JSON.stringify(e)}`)); }
        },
        error: (err: any) => { sub.unsubscribe(); reject(err); }
      });
      setTimeout(() => { sub.unsubscribe(); reject(new Error("timeout")); }, 60000);
    });
    console.log("SUCCESS!");
  } catch(e: any) {
    console.error("FAILED:", e.message?.slice(0, 300));
  }
  client.destroy();
}
main().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
