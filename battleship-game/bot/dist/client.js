import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { start } from "smoldot";
import { relayChainSpec, parachainSpec } from "./chainSpecs.js";
let smoldotInstance = null;
let relayChainInstance = null;
let clientInstance = null;
export async function getClient() {
    if (!clientInstance) {
        console.error("[DEBUG] getClient called");
        console.error(`[DEBUG] relayChainSpec type: ${typeof relayChainSpec}`);
        try {
            const parsed = JSON.parse(relayChainSpec);
            console.error(`[DEBUG] Parsed relay spec, bootNodes: ${parsed.bootNodes ? parsed.bootNodes.length : 'undefined'}`);
            if (parsed.bootNodes && parsed.bootNodes.length > 0) {
                console.error(`[DEBUG] First bootNode: ${parsed.bootNodes[0]}`);
            }
        }
        catch (e) {
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
export async function createNewClient(label) {
    const tag = label || "new";
    console.log(`[client:${tag}] Starting smoldot light client...`);
    const sm = start({
        maxLogLevel: 5,
        logCallback: (level, target, message) => {
            if (level <= 5) {
                const levelNames = ["", "ERROR", "WARN", "DEBUG"];
                console.log(`[smoldot:${tag}:${levelNames[level]}:${target}] ${message.slice(0, 300)}`);
            }
        },
    });
    console.log(`[client:${tag}] Adding relay chain...`);
    const relayChain = await sm.addChain({ chainSpec: relayChainSpec });
    console.log(`[client:${tag}] Adding parachain...`);
    const smProvider = getSmProvider(() => sm.addChain({
        chainSpec: parachainSpec,
        potentialRelayChains: [relayChain],
    }));
    console.log(`[client:${tag}] Creating PAPI client...`);
    const client = createClient(smProvider);
    console.log(`[client:${tag}] Waiting for first best block...`);
    const api = client.getUnsafeApi();
    await api.query.Battleship.NextGameId.getValue({ at: "best" });
    console.log(`[client:${tag}] Connected!`);
    return {
        client,
        destroy: () => {
            try {
                client.destroy();
            }
            catch { }
            try {
                sm.terminate();
            }
            catch { }
        },
    };
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
