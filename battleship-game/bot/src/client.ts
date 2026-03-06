import { createClient, type PolkadotClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { start, type Client as SmoldotClient, type Chain } from "smoldot";
import { relayChainSpec, parachainSpec } from "./chainSpecs.js";

let smoldotInstance: SmoldotClient | null = null;
let relayChainInstance: Chain | null = null;
let clientInstance: PolkadotClient | null = null;

export async function getClient(): Promise<PolkadotClient> {
  if (!clientInstance) {
    console.error("[DEBUG] getClient called");
    console.error(`[DEBUG] relayChainSpec type: ${typeof relayChainSpec}`);
    
    try {
      const parsed = JSON.parse(relayChainSpec);
      console.error(`[DEBUG] Parsed relay spec, bootNodes: ${parsed.bootNodes ? parsed.bootNodes.length : 'undefined'}`);
      if (parsed.bootNodes && parsed.bootNodes.length > 0) {
        console.error(`[DEBUG] First bootNode: ${parsed.bootNodes[0]}`);
      }
    } catch (e) {
      console.error(`[DEBUG] Failed to parse: ${e}`);
    }
    
    console.log("[client] Starting smoldot light client...");
    smoldotInstance = start({
      maxLogLevel: 4,
      logCallback: (level, target, message) => {
        const levelNames = ["", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
        if (level <= 3) {
          console.log(`[smoldot:${levelNames[level]}:${target}] ${message.slice(0, 300)}`);
        }
      },
    });

    console.log("[client] Adding relay chain...");
    relayChainInstance = await smoldotInstance.addChain({ chainSpec: relayChainSpec });

    console.log("[client] Adding parachain...");
    const smProvider = getSmProvider(() =>
      smoldotInstance!.addChain({
        chainSpec: parachainSpec,
        potentialRelayChains: [relayChainInstance!],
      }),
    );

    console.log("[client] Creating PAPI client with smoldot provider...");
    clientInstance = createClient(smProvider);

    console.log("[client] Waiting for first best block...");
    const api = clientInstance.getUnsafeApi() as any;
    const nextGameId = await api.query.Battleship.NextGameId.getValue({ at: "best" });
    console.log(`[client] Connected! NextGameId=${nextGameId}`);
  }
  
  return clientInstance;
}

export function disconnectClient(): void {
  if (clientInstance) {
    console.log("[client] Disconnecting...");
    clientInstance.destroy();
    clientInstance = null;
  }
  if (smoldotInstance) {
    smoldotInstance.terminate();
    smoldotInstance = null;
    relayChainInstance = null;
  }
}
