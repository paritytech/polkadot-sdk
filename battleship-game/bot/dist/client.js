import { createClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { start } from "smoldot";
import { relayChainSpec, parachainSpec } from "./chainSpecs.js";
let smoldotInstance = null;
let relayChainInstance = null;
let clientInstance = null;
let statementChainInstance = null;
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
    if (statementChainInstance)
        return statementChainInstance;
    if (!smoldotInstance || !relayChainInstance)
        return null;
    statementChainInstance = await smoldotInstance.addChain({
        chainSpec: parachainSpec,
        potentialRelayChains: [relayChainInstance],
    });
    return statementChainInstance;
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
        statementChainInstance = null;
    }
}
