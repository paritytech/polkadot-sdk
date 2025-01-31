import { createClient } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";

export function createPolkadotClient(endpoint, apiType) {
    const client = createClient(getWsProvider(endpoint));
    return client.getTypedApi(apiType);
}

export async function getFreeBalance(api, accountKey) {
	const balance = await api.query.System.Account.getValue(accountKey);
	return balance.data.free;
}
