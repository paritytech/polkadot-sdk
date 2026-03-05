import { createClient, type HexString } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws";

async function main() {
  const provider = getWsProvider("ws://127.0.0.1:35755");
  const client = createClient(provider);

  await new Promise(r => setTimeout(r, 3000));

  // Get metadata to inspect types
  const metadata = await client.getUnsafeApi().apis.Metadata.metadata_at_version(15);
  if (!metadata) { console.log("No v15 metadata"); process.exit(1); }
  
  // Parse to find commit_grid call type
  console.log("Looking for Battleship pallet in metadata...");
  
  // Just use runtime API to check the type definition
  // Check the actual on-chain metadata types for commit_grid
  const runtimeMetadata = await client.getUnsafeApi().apis.Metadata.metadata();
  console.log("Got metadata, type:", typeof runtimeMetadata);

  client.destroy();
  process.exit(0);
}

main().catch(e => { console.error(e); process.exit(1); });
