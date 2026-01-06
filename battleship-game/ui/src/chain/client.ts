import { createClient, type PolkadotClient } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";

const WS_URL = "ws://localhost:45115";

let clientInstance: PolkadotClient | null = null;

export async function getChainClient(): Promise<PolkadotClient> {
  if (!clientInstance) {
    clientInstance = createClient(
      withPolkadotSdkCompat(getWsProvider(WS_URL))
    );
  }
  return clientInstance;
}

export function disconnectClient(): void {
  if (clientInstance) {
    clientInstance.destroy();
    clientInstance = null;
  }
}
