import { createClient, type PolkadotClient } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { resetStatementStore } from "./statementStore.ts";

const DEFAULT_WS_URL = "ws://localhost:9944";

let rpcEndpoint: string = DEFAULT_WS_URL;
let clientInstance: PolkadotClient | null = null;
let clientReadyPromise: Promise<PolkadotClient> | null = null;

export function setRpcEndpoint(url: string): void {
  console.log("setRpcEndpoint called:", url, "current:", rpcEndpoint);
  if (url !== rpcEndpoint) {
    console.log("RPC endpoint changed, disconnecting old client");
    disconnectClient();
    rpcEndpoint = url;
  }
}

export function getRpcEndpoint(): string {
  return rpcEndpoint;
}

export async function getChainClient(): Promise<PolkadotClient> {
  if (!clientReadyPromise) {
    console.log("Creating new client with endpoint:", rpcEndpoint);
    const wsProvider = getWsProvider(rpcEndpoint);
    console.log("WS provider created");
    clientInstance = createClient(withPolkadotSdkCompat(wsProvider));
    console.log("Client created, checking runtimeToken...");

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const api = clientInstance.getUnsafeApi() as any;
    console.log("runtimeToken exists:", !!api.runtimeToken);
    if (api.runtimeToken) {
      const timeoutPromise = new Promise<never>((_, reject) => 
        setTimeout(() => reject(new Error("Client init timeout after 15s")), 15000)
      );
      clientReadyPromise = Promise.race([
        api.runtimeToken.then(() => {
          console.log("Client ready");
          return clientInstance!;
        }),
        timeoutPromise
      ]);
    } else {
      console.log("No runtimeToken, resolving immediately");
      clientReadyPromise = Promise.resolve(clientInstance!);
    }
  }
  return clientReadyPromise!;
}

export function disconnectClient(): void {
  if (clientInstance) {
    clientInstance.destroy();
    clientInstance = null;
    clientReadyPromise = null;
    resetStatementStore();
  }
}
