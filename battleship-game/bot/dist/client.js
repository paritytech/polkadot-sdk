import { start } from "smoldot";
import { getSmProvider } from "polkadot-api/sm-provider";
import { createClient } from "polkadot-api";
import { relayChainSpec, parachainSpec } from "./chainSpecs.js";
let smoldotInstance = null;
let relayChainInstance = null;
let clientInstance = null;
export async function getClient() {
    if (!clientInstance) {
        console.log("[client] Starting smoldot light client...");
        smoldotInstance = start({
            maxLogLevel: 3,
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
        const smProvider = getSmProvider(() => smoldotInstance.addChain({
            chainSpec: parachainSpec,
            potentialRelayChains: [relayChainInstance],
        }));
        console.log("[client] Creating PAPI client with smoldot provider...");
        clientInstance = createClient(smProvider);
        console.log("[client] Waiting for first best block...");
        const api = clientInstance.getUnsafeApi();
        const nextGameId = await api.query.Battleship.NextGameId.getValue({ at: "best" });
        console.log(`[client] Connected! NextGameId=${nextGameId}`);
    }
    return clientInstance;
}
export async function getStatementChain() {
    await getClient();
    if (!smoldotInstance || !relayChainInstance)
        return null;
    return smoldotInstance.addChain({
        chainSpec: parachainSpec,
        potentialRelayChains: [relayChainInstance],
    });
}
export function disconnectClient() {
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
