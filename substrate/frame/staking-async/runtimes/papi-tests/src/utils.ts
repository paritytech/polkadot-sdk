import { parachain, rc } from "@polkadot-api/descriptors";
import {
	Binary,
	createClient,
	type PolkadotClient,
	type PolkadotSigner,
	type TypedApi,
} from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { createLogger, format, log, transports } from "winston";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";

export const GlobalTimeout = 30 * 60 * 1000;

export const logger = createLogger({
	level: "debug",
	format: format.cli(),
	defaultMeta: { service: "staking-papi-tests" },
	transports: [new transports.Console()],
});

const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);
const aliceKeyPair = derive("//Alice");
export const alice = getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", aliceKeyPair.sign);

export type ApiDeclerations = {
	rcClient: PolkadotClient;
	paraClient: PolkadotClient;
	rcApi: TypedApi<typeof rc>;
	paraApi: TypedApi<typeof parachain>;
};

export async function nullifySigned(
	paraApi: TypedApi<typeof parachain>,
	signer: PolkadotSigner = alice
): Promise<void> {
	// signed and signed validation phase to 0
	const call = paraApi.tx.System.set_storage({
		items: [
			// SignedPhase key
			[
				Binary.fromBytes(
					Uint8Array.from([
						99, 88, 172, 210, 3, 94, 196, 187, 134, 63, 169, 129, 224, 193, 119, 185,
					])
				),
				Binary.fromBytes(Uint8Array.from([0])),
			],
			// SignedValidation key
			[
				Binary.fromBytes(
					Uint8Array.from([
						72, 56, 74, 129, 110, 79, 113, 169, 54, 203, 118, 220, 158, 48, 63, 42,
					])
				),
				Binary.fromBytes(Uint8Array.from([0])),
			],
		],
	}).decodedCall;
	const res = await paraApi.tx.Sudo.sudo({ call }).signAndSubmit(alice);
	logger.info("Set storage for nullify signed result:", res.ok);
}

export async function getApis(): Promise<ApiDeclerations> {
	const rcClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:9944")));
	const rcApi = rcClient.getTypedApi(rc);

	const paraClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:9946")));
	const paraApi = paraClient.getTypedApi(parachain);

	logger.info(`Connecting to relay chain ${(await rcApi.constants.System.Version()).spec_name}`);
	logger.info(`Connecting to parachain ${(await paraApi.constants.System.Version()).spec_name}`);

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
