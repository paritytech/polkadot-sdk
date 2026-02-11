import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { getWsProvider } from "polkadot-api/ws";
import { start } from "smoldot";
import * as fs from "fs";
import * as crypto from "crypto";

function hashMeta(meta: any): string {
  let bytes: Uint8Array;
  if (meta instanceof Uint8Array) bytes = meta;
  else if (meta?.asBytes) bytes = meta.asBytes();
  else if (meta?.inner) bytes = meta.inner;
  else bytes = new Uint8Array(Buffer.from(JSON.stringify(meta)));
  return crypto.createHash("md5").update(Buffer.from(bytes)).digest("hex") + ` (${bytes.length}b)`;
}

async function main() {
  // WS
  console.log("=== WS ===");
  const wsClient = createClient(getWsProvider("ws://localhost:37039"));
  await wsClient.getFinalizedBlock();
  const wsApi = wsClient.getUnsafeApi();
  const wsVersions = await wsApi.apis.Metadata.metadata_versions();
  console.log("versions:", wsVersions);
  for (const v of wsVersions) {
    const m = await wsApi.apis.Metadata.metadata_at_version(v);
    if (m != null) console.log(`  v${v}:`, typeof m, m?.constructor?.name, m?.length || "no length");
  }
  wsClient.destroy();

  // Smoldot
  console.log("\n=== Smoldot ===");
  const relaySpec = fs.readFileSync("public/chain-specs/relay.json", "utf-8");
  const paraSpec = fs.readFileSync("public/chain-specs/parachain.json", "utf-8");
  const smoldot = start({ maxLogLevel: 3 });
  const relayChain = await smoldot.addChain({ chainSpec: relaySpec });
  const smClient = createClient(getSmProvider(() => 
    smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] })
  ));
  await smClient.getFinalizedBlock();
  const smApi = smClient.getUnsafeApi();
  const smVersions = await smApi.apis.Metadata.metadata_versions();
  console.log("versions:", smVersions);
  for (const v of smVersions) {
    const m = await smApi.apis.Metadata.metadata_at_version(v);
    if (m != null) console.log(`  v${v}:`, typeof m, m?.constructor?.name, m?.length || "no length");
  }
  smClient.destroy();
  smoldot.terminate();
}

main().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
