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
import {
	DEV_PHRASE,
	entropyToMiniSecret,
	mnemonicToEntropy,
	type KeyPair,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";

export const GlobalTimeout = 45 * 60 * 1000;
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
const aliceStashKeyPair = derive("//Alice//stash");

export const alice = getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", aliceKeyPair.sign);
export const aliceStashSigner = getPolkadotSigner(
	aliceStashKeyPair.publicKey,
	"Sr25519",
	aliceStashKeyPair.sign,
);

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
	signer: PolkadotSigner = alice,
): Promise<boolean> {
	// signed and signed validation phase to 0
	const call = paraApi.tx.System.set_storage({
		items: [
			// SignedPhase key
			[
				Binary.fromBytes(
					Uint8Array.from([
						99, 88, 172, 210, 3, 94, 196, 187, 134, 63, 169, 129, 224, 193, 119, 185,
					]),
				),
				Binary.fromBytes(Uint8Array.from([0, 0, 0, 0])),
			],
			// SignedValidation key
			[
				Binary.fromBytes(
					Uint8Array.from([
						72, 56, 74, 129, 110, 79, 113, 169, 54, 203, 118, 220, 158, 48, 63, 42,
					]),
				),
				Binary.fromBytes(Uint8Array.from([0, 0, 0, 0])),
			],
		],
	}).decodedCall;
	const res = await paraApi.tx.Sudo.sudo({ call }).signAndSubmit(signer);
	return res.ok;
}

export async function nullifyUnsigned(
	paraApi: TypedApi<typeof parachain>,
	signer: PolkadotSigner = alice,
): Promise<boolean> {
	// signed and signed validation phase to 0
	const call = paraApi.tx.System.set_storage({
		items: [
			// UnsignedPhase key
			[
				Binary.fromBytes(
					Uint8Array.from([
						194, 9, 245, 216, 235, 146, 6, 129, 181, 108, 100, 184, 105, 78, 167, 140,
					]),
				),
				Binary.fromBytes(Uint8Array.from([0, 0, 0, 0])),
			],
		],
	}).decodedCall;
	const res = await paraApi.tx.Sudo.sudo({ call }).signAndSubmit(signer);
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
	const bigIntReplacer = (_key: string, value: any): any => {
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
			"Failed to stringify data due to unsupported types or circular references.",
		);
	}
}

import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

/// Session keys matching Westend relay chain configuration.
export interface SubstrateSessionKeys {
	grandpa: { publicKey: Uint8Array; sign: (msg: Uint8Array) => Uint8Array };
	babe: { publicKey: Uint8Array; sign: (msg: Uint8Array) => Uint8Array };
	paraValidator: { publicKey: Uint8Array; sign: (msg: Uint8Array) => Uint8Array };
	paraAssignment: { publicKey: Uint8Array; sign: (msg: Uint8Array) => Uint8Array };
	authorityDiscovery: { publicKey: Uint8Array; sign: (msg: Uint8Array) => Uint8Array };
	beefy: { publicKey: Uint8Array; sign: (msg: Uint8Array) => Uint8Array };
}

/// Generate session keys using @polkadot/keyring with proper Substrate signing context.
/// Uses different key types as required by Westend:
/// - grandpa: ed25519
/// - babe, para_validator, para_assignment, authority_discovery: sr25519
/// - beefy: ecdsa
export async function generateSessionKeys(uri: string): Promise<SubstrateSessionKeys> {
	await cryptoWaitReady();

	const sr25519Keyring = new Keyring({ type: "sr25519" });
	const ed25519Keyring = new Keyring({ type: "ed25519" });
	const ecdsaKeyring = new Keyring({ type: "ecdsa" });

	// Create keypairs from the URI (e.g., "//Alice//stash")
	const grandpaPair = ed25519Keyring.addFromUri(uri);
	const babePair = sr25519Keyring.addFromUri(uri);
	const paraValidatorPair = sr25519Keyring.addFromUri(uri);
	const paraAssignmentPair = sr25519Keyring.addFromUri(uri);
	const authorityDiscoveryPair = sr25519Keyring.addFromUri(uri);
	const beefyPair = ecdsaKeyring.addFromUri(uri);

	return {
		grandpa: {
			publicKey: grandpaPair.publicKey,
			sign: (msg: Uint8Array) => grandpaPair.sign(msg),
		},
		babe: {
			publicKey: babePair.publicKey,
			sign: (msg: Uint8Array) => babePair.sign(msg),
		},
		paraValidator: {
			publicKey: paraValidatorPair.publicKey,
			sign: (msg: Uint8Array) => paraValidatorPair.sign(msg),
		},
		paraAssignment: {
			publicKey: paraAssignmentPair.publicKey,
			sign: (msg: Uint8Array) => paraAssignmentPair.sign(msg),
		},
		authorityDiscovery: {
			publicKey: authorityDiscoveryPair.publicKey,
			sign: (msg: Uint8Array) => authorityDiscoveryPair.sign(msg),
		},
		beefy: {
			publicKey: beefyPair.publicKey,
			sign: (msg: Uint8Array) => beefyPair.sign(msg),
		},
	};
}

/// Encode session keys as SCALE bytes
export function encodeSessionKeys(keys: SubstrateSessionKeys): Uint8Array {
	// grandpa: 32 bytes ed25519
	// babe: 32 bytes sr25519
	// para_validator: 32 bytes sr25519
	// para_assignment: 32 bytes sr25519
	// authority_discovery: 32 bytes sr25519
	// beefy: 33 bytes ecdsa (compressed)
	const totalLength = 32 + 32 + 32 + 32 + 32 + 33;
	const encoded = new Uint8Array(totalLength);
	let offset = 0;

	encoded.set(keys.grandpa.publicKey, offset);
	offset += 32;
	encoded.set(keys.babe.publicKey, offset);
	offset += 32;
	encoded.set(keys.paraValidator.publicKey, offset);
	offset += 32;
	encoded.set(keys.paraAssignment.publicKey, offset);
	offset += 32;
	encoded.set(keys.authorityDiscovery.publicKey, offset);
	offset += 32;
	encoded.set(keys.beefy.publicKey, offset);

	return encoded;
}

/// Create ownership proof by signing the statement of ownership with each key.
/// The proof is a SCALE-encoded tuple of signatures:
/// - grandpa: ed25519 signature (64 bytes)
/// - babe: sr25519 signature (64 bytes)
/// - para_validator: sr25519 signature (64 bytes)
/// - para_assignment: sr25519 signature (64 bytes)
/// - authority_discovery: sr25519 signature (64 bytes)
/// - beefy: ecdsa signature (65 bytes)
export function createOwnershipProof(keys: SubstrateSessionKeys, owner: Uint8Array): Uint8Array {
	// Substrate's statement_of_ownership prefixes with "POP_"
	const POP_PREFIX = new TextEncoder().encode("POP_");
	const statement = new Uint8Array(POP_PREFIX.length + owner.length);
	statement.set(POP_PREFIX, 0);
	statement.set(owner, POP_PREFIX.length);

	const grandpaSig = keys.grandpa.sign(statement);
	const babeSig = keys.babe.sign(statement);
	const paraValidatorSig = keys.paraValidator.sign(statement);
	const paraAssignmentSig = keys.paraAssignment.sign(statement);
	const authorityDiscoverySig = keys.authorityDiscovery.sign(statement);
	const beefySig = keys.beefy.sign(statement);

	// Total: 64 + 64 + 64 + 64 + 64 + 65 = 385 bytes
	const totalLength = 64 + 64 + 64 + 64 + 64 + 65;
	const proof = new Uint8Array(totalLength);
	let offset = 0;

	proof.set(grandpaSig, offset);
	offset += 64;
	proof.set(babeSig, offset);
	offset += 64;
	proof.set(paraValidatorSig, offset);
	offset += 64;
	proof.set(paraAssignmentSig, offset);
	offset += 64;
	proof.set(authorityDiscoverySig, offset);
	offset += 64;
	proof.set(beefySig, offset);

	return proof;
}

/// Convert SS58 address to raw 32-byte AccountId.
/// This is a simplified version - for test accounts we can derive directly.
export function accountIdBytes(derivationPath: string): Uint8Array {
	const secret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
	const derive = sr25519CreateDerive(secret);
	const keyPair = derive(derivationPath);
	return keyPair.publicKey;
}
