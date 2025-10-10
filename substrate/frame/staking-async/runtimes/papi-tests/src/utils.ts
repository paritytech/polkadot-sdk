import { parachain, rc } from "@polkadot-api/descriptors";
import {
	Binary,
	createClient,
	type PolkadotClient,
	type PolkadotSigner,
	type TypedApi,
} from "polkadot-api";
import { fromBufferToBase58 } from "@polkadot-api/substrate-bindings";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { createLogger, format, transports } from "winston";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy, type KeyPair } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";

export const GlobalTimeout = 30 * 60 * 1000;
export const aliceStash = "5GNJqTPyNqANBkUVMN1LPPrxXnFouWXoe2wNSmmEoLctxiZY";


export const logger = createLogger({
	level: process.env.LOG_LEVEL || "verbose",
	format: format.combine(format.timestamp(), format.cli()),
	defaultMeta: { service: "staking-papi-tests" },
	transports: [new transports.Console()],
});

const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);
const aliceKeyPair = derive("//Alice");

export const alice = getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", aliceKeyPair.sign);

export function deriveFrom(s: string, d: string): KeyPair {
	const miniSecret = entropyToMiniSecret(mnemonicToEntropy(s));
	const derive = sr25519CreateDerive(miniSecret);
	return derive(d);
}

export function derivePubkeyFrom(d: string): string {
	const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
	const derive = sr25519CreateDerive(miniSecret);
	const keyPair = derive(d);
	// Convert to SS58 address using Substrate format (42)
	return ss58(keyPair.publicKey);
}

export function ss58(key: Uint8Array): string {
	return fromBufferToBase58(42)(key);
}

export type ApiDeclarations = {
	rcClient: PolkadotClient;
	paraClient: PolkadotClient;
	rcApi: TypedApi<typeof rc>;
	paraApi: TypedApi<typeof parachain>;
};

export async function nullifySigned(
	paraApi: TypedApi<typeof parachain>,
	signer: PolkadotSigner = alice
): Promise<boolean> {
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
				Binary.fromBytes(Uint8Array.from([0, 0, 0, 0])),
			],
			// SignedValidation key
			[
				Binary.fromBytes(
					Uint8Array.from([
						72, 56, 74, 129, 110, 79, 113, 169, 54, 203, 118, 220, 158, 48, 63, 42,
					])
				),
				Binary.fromBytes(Uint8Array.from([0, 0, 0, 0])),
			],
		],
	}).decodedCall;
	const res = await paraApi.tx.Sudo.sudo({ call }).signAndSubmit(alice);
	return res.ok;
}

export async function nullifyUnsigned(
	paraApi: TypedApi<typeof parachain>,
	signer: PolkadotSigner = alice
): Promise<boolean> {
	// signed and signed validation phase to 0
	const call = paraApi.tx.System.set_storage({
		items: [
			// UnsignedPhase key
			[
				Binary.fromBytes(
					Uint8Array.from([
						194, 9, 245, 216, 235, 146, 6, 129, 181, 108, 100, 184, 105, 78, 167, 140,
					])
				),
				Binary.fromBytes(Uint8Array.from([0, 0, 0, 0])),
			],
		],
	}).decodedCall;
	const res = await paraApi.tx.Sudo.sudo({ call }).signAndSubmit(alice);
	return res.ok;
}

export async function getApis(): Promise<ApiDeclarations> {
	const rcClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:9945")));
	const rcApi = rcClient.getTypedApi(rc);

	const paraClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:9946")));
	const paraApi = paraClient.getTypedApi(parachain);

	logger.info(`Connected to ${(await rcApi.constants.System.Version()).spec_name}`);
	logger.info(`Connected to ${(await paraApi.constants.System.Version()).spec_name}`);

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
