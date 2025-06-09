import { parachain, rc } from "@polkadot-api/descriptors";
import { createClient } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { createLogger, format, transports } from "winston";

export const GlobalTimeout = 30 * 60 * 1000;

export const logger = createLogger({
	level: "debug",
	format: format.cli(),
	defaultMeta: { service: "staking-papi-tests" },
	transports: [new transports.Console()],
});

export function getApis(): {
	rcClient: typeof rcClient;
	paraClient: typeof paraClient;
	rcApi: typeof rcApi;
	paraApi: typeof paraApi;
} {
	const rcClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:9944")));
	const rcApi = rcClient.getTypedApi(rc);

	const paraClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:9946")));
	const paraApi = paraClient.getTypedApi(parachain);

	return { rcApi, paraApi, rcClient, paraClient };
}

// Safely convert anything to a string so we can compare them.
export function safeJsonStringify(data: any): string {
	const bigIntReplacer = (key: string, value: any): any => {
		if (typeof value === "bigint") {
			return value.toString();
		}
		return value;
	};

	try {
		return JSON.stringify(data, bigIntReplacer);
	} catch (error: any) {
		// Handle potential errors during stringification (e.g., circular references)
		console.error("Error during JSON stringification:", error.message);
		throw new Error(
			"Failed to stringify data due to unsupported types or circular references."
		);
	}
}
