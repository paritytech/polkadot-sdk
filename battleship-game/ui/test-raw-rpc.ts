import { start } from "smoldot";
import * as fs from "fs";

async function main() {
  const relaySpec = fs.readFileSync("public/chain-specs/relay.json", "utf-8");
  const paraSpec = fs.readFileSync("public/chain-specs/parachain.json", "utf-8");
  
  const smoldot = start({ maxLogLevel: 3 });
  const relayChain = await smoldot.addChain({ chainSpec: relaySpec });
  const paraChain = await smoldot.addChain({ chainSpec: paraSpec, potentialRelayChains: [relayChain] });
  
  console.log("Chains added, waiting 10s for sync...");
  await new Promise(r => setTimeout(r, 10000));
  
  // Test 1: chain_getBlockHash (legacy API)
  paraChain.sendJsonRpc('{"jsonrpc":"2.0","id":1,"method":"chain_getBlockHash","params":[0]}');
  const resp1 = await paraChain.nextJsonRpcResponse();
  console.log("chain_getBlockHash(0):", resp1);
  
  // Test 2: chain_getFinalizedHead (legacy API)  
  paraChain.sendJsonRpc('{"jsonrpc":"2.0","id":2,"method":"chain_getFinalizedHead","params":[]}');
  const resp2 = await paraChain.nextJsonRpcResponse();
  console.log("chain_getFinalizedHead:", resp2);
  
  // Test 3: chainHead_v1_follow (new API - what PAPI 2.0 uses)
  paraChain.sendJsonRpc('{"jsonrpc":"2.0","id":3,"method":"chainHead_v1_follow","params":[true]}');
  
  // Collect responses for 10 seconds
  const deadline = Date.now() + 10000;
  let responseCount = 0;
  while (Date.now() < deadline) {
    try {
      const resp = await Promise.race([
        paraChain.nextJsonRpcResponse(),
        new Promise<string>((_, reject) => setTimeout(() => reject("timeout"), 2000))
      ]);
      responseCount++;
      const parsed = JSON.parse(resp as string);
      if (parsed.id) {
        console.log(`chainHead_v1_follow response (id=${parsed.id}):`, JSON.stringify(parsed).slice(0, 200));
      } else if (parsed.params?.subscription) {
        const event = parsed.params?.result;
        console.log(`chainHead notification: ${event?.event || 'unknown'}`, JSON.stringify(event).slice(0, 200));
      } else {
        console.log(`Unknown response:`, JSON.stringify(parsed).slice(0, 200));
      }
    } catch {
      break;
    }
  }
  console.log(`Total chainHead responses: ${responseCount}`);
  
  smoldot.terminate();
  process.exit(0);
}

main().catch(e => { console.error(e); process.exit(1); });
