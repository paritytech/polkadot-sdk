import { createClient, type PolkadotClient } from "polkadot-api";
import { getSmProvider } from "polkadot-api/sm-provider";
import { firstValueFrom } from "rxjs";
import { start, type Client as SmoldotClient, type Chain } from "smoldot";
import { relayChainSpec, parachainSpec } from "./chainSpecs.ts";

let smoldotInstance: SmoldotClient | null = null;
let relayChainInstance: Chain | null = null;
let clientInstance: PolkadotClient | null = null;
let clientReadyPromise: Promise<PolkadotClient> | null = null;
let proxyWs: WebSocket | null = null;

function getProxyUrl(): string | null {
	try {
		const params = new URLSearchParams(window.location.search);
		const url = params.get("smoldotProxy");
		console.log(
			`[client] getProxyUrl: search=${window.location.search}, smoldotProxy=${url}`,
		);
		return url;
	} catch (e) {
		console.log(`[client] getProxyUrl error:`, e);
		return null;
	}
}

function initSmoldot(): SmoldotClient {
	if (!smoldotInstance) {
		console.log("Starting smoldot light client...");
		smoldotInstance = start({
			maxLogLevel: 4,
			logCallback: (level, target, message) => {
				const levelNames = ["", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
				if (level <= 3) {
					console.log(`[smoldot:${levelNames[level]}:${target}] ${message.slice(0, 300)}`);
				}
			},
		});
	}
	return smoldotInstance;
}

// Direct WebSocket JsonRpcProvider — no getProxy/getSyncProvider reconnection.
// getWsProvider uses getProxy which has heartbeat-based reconnection that causes
// duplicate chainHead_v1_follow requests and "Initialized event out of order" errors.
function getDirectWsProvider(
	url: string,
): (onMessage: (msg: any) => void) => { send: (msg: any) => void; disconnect: () => void } {
	return (onMessage) => {
		const ws = new WebSocket(url);
		proxyWs = ws;
		const pending: any[] = [];
		let ready = false;

		ws.onopen = () => {
			console.log("[direct-ws] Connected to proxy");
			ready = true;
			for (const msg of pending) {
				ws.send(JSON.stringify(msg));
			}
			pending.length = 0;
		};

		ws.onmessage = (e) => {
			try {
				onMessage(JSON.parse(e.data as string));
			} catch (err) {
				console.error("[direct-ws] Parse error:", err);
			}
		};

		ws.onerror = (e) => {
			console.error("[direct-ws] WS error:", e);
		};

		ws.onclose = () => {
			console.warn("[direct-ws] WS closed");
		};

		return {
			send(msg: any) {
				if (ready && ws.readyState === WebSocket.OPEN) {
					ws.send(JSON.stringify(msg));
				} else {
					pending.push(msg);
				}
			},
			disconnect() {
				ws.close();
			},
		};
	};
}

export async function getChainClient(): Promise<PolkadotClient> {
	if (!clientReadyPromise) {
		clientReadyPromise = (async () => {
			const proxyUrl = getProxyUrl();

			if (proxyUrl) {
				console.log(`Connecting to smoldot proxy via direct WS: ${proxyUrl}`);
				const provider = getDirectWsProvider(proxyUrl);
				console.log("Creating PAPI client with direct WS provider...");
				clientInstance = createClient(provider);
			} else {
				const smoldot = initSmoldot();

				console.log("Adding relay chain...");
				relayChainInstance = await smoldot.addChain({ chainSpec: relayChainSpec });

				console.log("Adding parachain...");
				const smProvider = getSmProvider(() =>
					smoldot.addChain({
						chainSpec: parachainSpec,
						potentialRelayChains: [relayChainInstance!],
					}),
				);

				console.log("Creating PAPI client with smoldot provider...");
				clientInstance = createClient(smProvider);
			}

			console.log("Waiting for first best block...");
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const bestBlock = await firstValueFrom(clientInstance.bestBlocks$ as any);
			console.log("Got first best block:", bestBlock);
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const api = clientInstance.getUnsafeApi() as any;
			console.log("Querying NextGameId at best...");
			const nextGameId = await api.query.Battleship.NextGameId.getValue({
				at: "best",
			});
			console.log(`Runtime ready, NextGameId=${nextGameId}`);

			console.log("Light client ready");
			return clientInstance;
		})();
	}
	return clientReadyPromise;
}

export function disconnectClient(): void {
	if (clientInstance) {
		clientInstance.destroy();
		clientInstance = null;
		clientReadyPromise = null;
	}
	if (proxyWs) {
		proxyWs.close();
		proxyWs = null;
	}
	if (smoldotInstance) {
		smoldotInstance.terminate();
		smoldotInstance = null;
		relayChainInstance = null;
	}
}
